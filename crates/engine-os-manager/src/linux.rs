//! Linux adapters. All observations are read-only; RTC writing is a separate,
//! fixed-device operation invoked only after a verified provider completion.
use crate::time::{ClockReading, ClockReceipt, ProviderIdentity, RtcBootReading, SyncObservation};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::time::Duration;
use zbus::zvariant::OwnedValue;

const PROVIDER: &str = "org.freedesktop.timesync1";
const INTERFACE: &str = "org.freedesktop.timesync1.Manager";
const OBJECT: &str = "/org/freedesktop/timesync1";
const UNIT: &str = "/org/freedesktop/systemd1/unit/systemd_2dtimesyncd_2eservice";
const DEADLINE: Duration = Duration::from_secs(5);

pub fn boot_id() -> Result<String> {
    let value = fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    let value = value.trim();
    if value.len() != 36 || !value.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
        return Err(Error::Invalid("invalid Linux boot identity".into()));
    }
    Ok(value.into())
}

fn clock_us(id: libc::clockid_t) -> Result<u64> {
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: value is writable and the caller supplies a supported clock id.
    if unsafe { libc::clock_gettime(id, &mut value) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    u64::try_from(value.tv_sec)
        .ok()
        .and_then(|secs| secs.checked_mul(1_000_000))
        .and_then(|us| us.checked_add(value.tv_nsec as u64 / 1_000))
        .ok_or_else(|| Error::Invalid("system clock is outside supported range".into()))
}

pub fn clock_reading() -> Result<ClockReading> {
    Ok(ClockReading {
        boottime_us: clock_us(libc::CLOCK_BOOTTIME)?,
        monotonic_us: clock_us(libc::CLOCK_MONOTONIC)?,
        realtime_us: clock_us(libc::CLOCK_REALTIME)?,
    })
}

fn kernel_synchronized() -> Result<bool> {
    // SAFETY: all-zero timex with modes=0 is the documented read-only query.
    let mut value: libc::timex = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::adjtimex(&mut value) };
    if result < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(value.status & (libc::STA_UNSYNC | libc::STA_CLOCKERR) == 0 && result != libc::TIME_ERROR)
}

type NtpMessage = (
    u32,
    u32,
    u32,
    u32,
    i32,
    u64,
    u64,
    Vec<u8>,
    u64,
    u64,
    u64,
    u64,
    bool,
    u64,
    u64,
);

async fn identity(connection: &zbus::Connection) -> std::result::Result<ProviderIdentity, String> {
    let bus = zbus::fdo::DBusProxy::new(connection)
        .await
        .map_err(explain)?;
    let owner = bus
        .get_name_owner(PROVIDER.try_into().map_err(explain)?)
        .await
        .map_err(explain)?;
    let pid = bus
        .get_connection_unix_process_id(owner.clone().into())
        .await
        .map_err(explain)?;
    let uid = bus
        .get_connection_unix_user(owner.clone().into())
        .await
        .map_err(explain)?;
    let unit = zbus::Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        UNIT,
        "org.freedesktop.systemd1.Unit",
    )
    .await
    .map_err(explain)?;
    let service = zbus::Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        UNIT,
        "org.freedesktop.systemd1.Service",
    )
    .await
    .map_err(explain)?;
    let active: String = unit.get_property("ActiveState").await.map_err(explain)?;
    let invocation: Vec<u8> = unit.get_property("InvocationID").await.map_err(explain)?;
    let main_pid: u32 = service.get_property("MainPID").await.map_err(explain)?;
    if active != "active"
        || invocation.len() != 16
        || invocation.iter().all(|b| *b == 0)
        || pid == 0
        || main_pid != pid
    {
        return Err("time provider identity is not active and coherent".into());
    }
    Ok(ProviderIdentity {
        bus_owner: owner.to_string(),
        invocation_id: invocation
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        pid,
        uid,
    })
}

async fn properties(
    connection: &zbus::Connection,
    owner: &str,
) -> std::result::Result<(ClockReceipt, NtpMessage), String> {
    // Use GetAll explicitly, without proxy caching, to read both values in one
    // provider dispatch. Pin the unique bus owner rather than a reusable name.
    let proxy = zbus::Proxy::new(connection, owner, OBJECT, "org.freedesktop.DBus.Properties")
        .await
        .map_err(explain)?;
    let mut values: std::collections::HashMap<String, OwnedValue> =
        proxy.call("GetAll", &(INTERFACE,)).await.map_err(explain)?;
    let receipt = values.remove("SpacewarsLastSuccessfulSync").ok_or(
        "time provider lacks the successful-correction receipt (systemd image patch required)",
    )?;
    let (packet_count, realtime_us, monotonic_us, boottime_us): (u64, u64, u64, u64) =
        receipt.try_into().map_err(explain)?;
    let ntp = values
        .remove("NTPMessage")
        .ok_or("time provider omitted NTPMessage")?
        .try_into()
        .map_err(explain)?;
    Ok((
        ClockReceipt {
            packet_count,
            realtime_us,
            monotonic_us,
            boottime_us,
        },
        ntp,
    ))
}

async fn observe_connection(
    connection: &zbus::Connection,
    clock: impl FnOnce(
        &ProviderIdentity,
    ) -> std::result::Result<(Option<u64>, bool, ClockReading), String>,
) -> std::result::Result<SyncObservation, String> {
    let before = identity(connection).await?;
    let (receipt, ntp) = properties(connection, &before.bus_owner).await?;
    let (notification, kernel_synchronized, now) = clock(&before)?;
    let (after_receipt, after_ntp) = properties(connection, &before.bus_owner).await?;
    let after = identity(connection).await?;
    // The immutable packet identity also catches a new rejected/spike
    // packet which leaves the last-successful receipt unchanged.
    if before != after
        || receipt != after_receipt
        || (ntp.8, ntp.11, ntp.12, ntp.13)
            != (after_ntp.8, after_ntp.11, after_ntp.12, after_ntp.13)
    {
        return Err("time provider changed during observation; retrying".into());
    }
    Ok(SyncObservation {
        provider: before,
        receipt,
        ntp_packet_count: ntp.13,
        ntp_spike: ntp.12,
        ntp_valid: ntp.0 < 3
            && (3..=4).contains(&ntp.1)
            && ntp.2 == 4
            && (1..16).contains(&ntp.3)
            && ntp.8 > 0
            && ntp.9 > 0
            && ntp.10 > 0
            && ntp.11 > 0,
        notification_realtime_us: notification,
        kernel_synchronized,
        now,
    })
}

/// Every complete connection/read/correlation attempt has one five-second
/// deadline, including DBus setup and provider identity revalidation.
pub fn observe_sync() -> std::result::Result<SyncObservation, String> {
    futures_lite::future::block_on(futures_lite::future::or(
        async {
            let connection = zbus::connection::Builder::system()
                .map_err(explain)?
                .method_timeout(DEADLINE)
                .build()
                .await
                .map_err(explain)?;
            observe_connection(&connection, |provider| {
                let notification = fs::symlink_metadata("/run/systemd/timesync/synchronized")
                    .ok()
                    .filter(|metadata| {
                        metadata.is_file()
                            && metadata.uid() == provider.uid
                            && metadata.mode() & 0o022 == 0
                    })
                    .and_then(|metadata| {
                        u64::try_from(metadata.mtime())
                            .ok()
                            .and_then(|seconds| seconds.checked_mul(1_000_000))
                            .and_then(|us| us.checked_add(metadata.mtime_nsec() as u64 / 1_000))
                    });
                Ok((
                    notification,
                    kernel_synchronized().map_err(explain)?,
                    clock_reading().map_err(explain)?,
                ))
            })
            .await
        },
        async {
            async_io::Timer::after(DEADLINE).await;
            Err("time provider observation timed out after five seconds".into())
        },
    ))
}

fn explain(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct RtcTime {
    sec: libc::c_int,
    min: libc::c_int,
    hour: libc::c_int,
    mday: libc::c_int,
    mon: libc::c_int,
    year: libc::c_int,
    wday: libc::c_int,
    yday: libc::c_int,
    isdst: libc::c_int,
}
nix::ioctl_read!(rtc_read_time, b'p', 0x09, RtcTime);
nix::ioctl_write_ptr!(rtc_set_time, b'p', 0x0a, RtcTime);

fn rtc_driver() -> Option<String> {
    fs::read_to_string("/sys/class/rtc/rtc0/name")
        .ok()
        // Linux reports e.g. "rpi-rtc soc:rpi_rtc"; the suffix is the device.
        .and_then(|name| name.split_whitespace().next().map(str::to_owned))
}

fn rtc_open() -> Result<File> {
    if rtc_driver().as_deref() != Some("rpi-rtc") {
        return Err(Error::Invalid("only the Pi 5 rpi-rtc is supported".into()));
    }
    Ok(OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open("/dev/rtc0")?)
}

fn rtc_read(file: &File) -> Result<u64> {
    let mut rtc = RtcTime::default();
    // SAFETY: file is an opened RTC and rtc has Linux struct rtc_time layout.
    unsafe { rtc_read_time(file.as_raw_fd(), &mut rtc) }
        .map_err(|error| Error::Invalid(error.to_string()))?;
    // RTC fields are UTC; timegm performs no timezone/DST conversion.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    tm.tm_sec = rtc.sec;
    tm.tm_min = rtc.min;
    tm.tm_hour = rtc.hour;
    tm.tm_mday = rtc.mday;
    tm.tm_mon = rtc.mon;
    tm.tm_year = rtc.year;
    let seconds = unsafe { libc::timegm(&mut tm) };
    if seconds < 0
        || tm.tm_sec != rtc.sec
        || tm.tm_min != rtc.min
        || tm.tm_hour != rtc.hour
        || tm.tm_mday != rtc.mday
        || tm.tm_mon != rtc.mon
        || tm.tm_year != rtc.year
    {
        return Err(Error::Invalid("invalid RTC calendar fields".into()));
    }
    (seconds as u64)
        .checked_mul(1_000_000)
        .ok_or_else(|| Error::Invalid("RTC overflow".into()))
}

/// Ordered before timesyncd in the image. Reading the actual device avoids
/// confusing PID 1's saved system-clock restoration with retained RTC time.
pub fn capture_rtc() -> Result<RtcBootReading> {
    let driver = rtc_driver();
    let utc_us = rtc_open().and_then(|file| rtc_read(&file)).ok();
    Ok(RtcBootReading {
        boot_id: boot_id()?,
        driver,
        utc_us,
        boottime_us: clock_us(libc::CLOCK_BOOTTIME)?,
        // The supported 6.6 rpi-rtc driver has no voltage-loss/reset ioctl.
        // Do not infer retention until the board/firmware procedure is qualified.
        retention_established: false,
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RtcWriteResult {
    pub driver: String,
    pub verified_utc_us: u64,
}

/// Run only in a bounded child process: a stuck driver cannot stall observation
/// or the socket service. No battery charging or alarm control is performed.
pub fn write_rtc(receipt: ClockReceipt) -> Result<RtcWriteResult> {
    let file = rtc_open()?;
    let before = clock_reading()?;
    if !receipt.matches_running_clock(before) || !kernel_synchronized()? {
        return Err(Error::Invalid(
            "system clock no longer matches the accepted synchronization".into(),
        ));
    }
    let seconds = (before.realtime_us / 1_000_000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::gmtime_r(&seconds, &mut tm) }.is_null() {
        return Err(Error::Invalid("system UTC is outside RTC range".into()));
    }
    let rtc = RtcTime {
        sec: tm.tm_sec,
        min: tm.tm_min,
        hour: tm.tm_hour,
        mday: tm.tm_mday,
        mon: tm.tm_mon,
        year: tm.tm_year,
        wday: tm.tm_wday,
        yday: tm.tm_yday,
        isdst: 0,
    };
    unsafe { rtc_set_time(file.as_raw_fd(), &rtc) }
        .map_err(|error| Error::Invalid(error.to_string()))?;
    let verified_utc_us = rtc_read(&file)?;
    let after = clock_reading()?;
    if verified_utc_us.abs_diff(after.realtime_us) > 1_500_000
        || !receipt.matches_running_clock(after)
        || !kernel_synchronized()?
    {
        return Err(Error::Invalid(
            "RTC write readback differs from system UTC".into(),
        ));
    }
    Ok(RtcWriteResult {
        driver: "rpi-rtc".into(),
        verified_utc_us,
    })
}

#[cfg(test)]
#[path = "linux_tests.rs"]
mod tests;
