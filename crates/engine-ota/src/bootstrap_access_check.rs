//! Bounded read-only checks after the fixed fallback-root replacements.
//! Configuration checks do not attest a successful SSH login or authorize boot.
use super::{invalid, legacy_files};
use crate::file_command::FileCommand;
use crate::{Result, Sha256};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::net::IpAddr;
use std::process::Command;
use std::time::Duration;

const LIMIT: usize = 64 * 1024;
// Reviewed fixed-unit, bounded, no-pager helper. A different legacy helper
// requires review; its sudo grant alone is not sufficient evidence of safety.
const LOG_HELPER_SHA256: &str = "91edfe98c18c18febb1b6aa726447f710fedb1498c7a78308f532d1b413032c7";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SshCheckConnection {
    client_address: IpAddr,
    client_port: u16,
    server_address: IpAddr,
    server_port: u16,
}

impl SshCheckConnection {
    /// The four fields of SSH_CONNECTION, supplied by the root operator.
    /// This is a test context only, never proof of operator authentication.
    pub fn parse(text: &str) -> Result<Self> {
        let fields: Vec<_> = text.split_ascii_whitespace().collect();
        if text.len() > 256 || fields.len() != 4 {
            return Err(invalid("expected four SSH connection address/port fields"));
        }
        let address = |value: &str| {
            value
                .parse::<IpAddr>()
                .map_err(|_| invalid("invalid SSH check address"))
        };
        let port = |value: &str| -> Result<u16> {
            let port = value
                .parse()
                .map_err(|_| invalid("invalid SSH check port"))?;
            if port == 0 {
                return Err(invalid("invalid SSH check port"));
            }
            Ok(port)
        };
        Ok(Self {
            client_address: address(fields[0])?,
            client_port: port(fields[1])?,
            server_address: address(fields[2])?,
            server_port: port(fields[3])?,
        })
    }

    fn argument(&self, user: &str) -> String {
        // Checks below require UseDNS=no, so the hostname is the peer address.
        format!(
            "user={user},addr={},host={},laddr={},lport={}",
            self.client_address, self.client_address, self.server_address, self.server_port
        )
    }
}

#[derive(Debug, Serialize)]
pub struct LegacyAccessCheck {
    pub schema: u32,
    pub scope: &'static str,
    pub package_sha256: Sha256,
    pub raw_copies_sha256: Sha256,
    pub data_receipt_sha256: Sha256,
    pub originals_sha256: Sha256,
    pub replacement_journal_sha256: Sha256,
    pub sudo_policy_sha256: Sha256,
    pub permitted_sudo_commands: usize,
    pub ssh_connection: SshCheckConnection,
    pub root_sshd_configuration_sha256: Sha256,
    pub runtime_sshd_configuration_sha256: Sha256,
    pub checked_services: Vec<String>,
    pub independent_logins_verified: bool,
    pub equivalent_privilege_audit_complete: bool,
    pub ready_for_trial: bool,
}

pub(super) struct Observations {
    pub(super) sudo_sha256: Sha256,
    pub(super) commands: usize,
    pub(super) root_sshd: Sha256,
    pub(super) runtime_sshd: Sha256,
    pub(super) services: Vec<String>,
}

pub(super) fn check(
    root: &File,
    data: &File,
    connection: &SshCheckConnection,
) -> Result<Observations> {
    let mut runner = FileCommand::default();
    let mut run = |program: &str, args: &[&str]| -> Result<Vec<u8>> {
        let mut command = Command::new(program);
        command.args(args);
        runner.run(command, data, Duration::from_secs(10), LIMIT)
    };
    // Validate the unmodified helper behind the only remaining wildcard grant.
    let logs = fixed_file(root, "usr/sbin/spacewars-logs", false)?
        .ok_or_else(|| invalid("missing reviewed log helper"))?;
    if logs.contents.sha256.to_string() != LOG_HELPER_SHA256
        || logs.identity.mode != 0o755
        || logs.identity.gid != 0
        || !logs.attributes.is_empty()
    {
        return Err(invalid(
            "legacy log helper differs from its reviewed implementation",
        ));
    }
    for path in [
        "etc/sudoers",
        "etc/sudoers.d/spacewars-logs",
        "etc/sudoers.d/spacewars-renderer",
    ] {
        if let Some(file) = fixed_file(root, path, path.ends_with("spacewars-renderer"))?
            && (file.identity.mode != 0o440 || file.identity.gid != 0)
        {
            return Err(invalid("unsafe sudo policy permissions"));
        }
    }
    run("/usr/sbin/visudo", &["-c", "-s"])?;
    let sudo = run("/usr/bin/sudo", &["-n", "-ll", "-U", "spacewars"])?;
    let commands = sudo_policy(text(&sudo)?)?;

    let mut services = Vec::new();
    let mut ssh_modes = 0;
    for (unit, mode) in [
        ("sshd.service", "-D"),
        ("sshd@spacewars-bootstrap-check.service", "-i"),
    ] {
        let snapshot = service(&mut run, unit)?;
        if snapshot.get("LoadState").map(String::as_str) == Some("not-found") {
            continue;
        }
        ssh_service(&snapshot, mode)?;
        services.push(unit.to_owned());
        ssh_modes += 1;
    }
    if ssh_modes == 0 {
        return Err(invalid("no supported SSH service is loaded"));
    }
    if let Some(defaults) = file_bytes(root, "etc/default/ssh", true)? {
        ssh_environment(text(&defaults)?)?;
    }
    fixed_file(root, "etc/ssh/sshd_config", false)?;
    // -T performs syntax and host-key checks. No daemon is started and no keys
    // or raw configuration text are included in the public result or errors.
    let root_config = run(
        "/usr/sbin/sshd",
        &["-T", "-C", &connection.argument("root")],
    )?;
    ssh_configuration(text(&root_config)?)?;
    let runtime_config = run(
        "/usr/sbin/sshd",
        &["-T", "-C", &connection.argument("spacewars")],
    )?;
    ssh_configuration(text(&runtime_config)?)?;
    for unit in [
        "persistent-data-init.service",
        "persistent-data-bind.service",
        "spacewars-data-init.service",
        "spacewars-access.service",
        "NetworkManager.service",
    ] {
        let snapshot = service(&mut run, unit)?;
        persistence_service(&snapshot, unit)?;
        services.push(unit.to_owned());
    }
    Ok(Observations {
        sudo_sha256: Sha256::digest(&sudo),
        commands,
        root_sshd: Sha256::digest(&root_config),
        runtime_sshd: Sha256::digest(&runtime_config),
        services,
    })
}

fn text(bytes: &[u8]) -> Result<&str> {
    if bytes.len() > LIMIT
        || !bytes.is_ascii()
        || bytes.iter().any(|b| *b < 32 && !b"\t\n\r".contains(b))
    {
        return Err(invalid("invalid access-check output"));
    }
    std::str::from_utf8(bytes).map_err(|_| invalid("invalid access-check output"))
}

fn fixed_file(root: &File, path: &str, optional: bool) -> Result<Option<legacy_files::Original>> {
    let (parent, component) = path.rsplit_once('/').expect("fixed check path");
    let result = if let Some(parent) = legacy_files::directory(root, parent, 0)? {
        legacy_files::file_at(
            root,
            &parent,
            component,
            0,
            LIMIT as u64,
            &mut std::io::sink(),
        )?
    } else {
        None
    };
    if !optional && result.is_none() {
        return Err(invalid("missing required access configuration file"));
    }
    Ok(result)
}

fn file_bytes(root: &File, path: &str, optional: bool) -> Result<Option<Vec<u8>>> {
    let (parent, component) = path.rsplit_once('/').expect("fixed check path");
    let Some(parent) = legacy_files::directory(root, parent, 0)? else {
        return if optional {
            Ok(None)
        } else {
            Err(invalid("missing access configuration parent"))
        };
    };
    let mut bytes = Vec::new();
    let present =
        legacy_files::file_at(root, &parent, component, 0, LIMIT as u64, &mut bytes)?.is_some();
    if !optional && !present {
        return Err(invalid("missing access configuration file"));
    }
    Ok(present.then_some(bytes))
}

/// Accept only the supported effective listing, not general sudoers syntax.
/// Unknown defaults, tags, runas identities, commands or formatting fail closed.
fn sudo_policy(output: &str) -> Result<usize> {
    let lines: Vec<_> = output
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let header = lines
        .iter()
        .position(|s| {
            s.starts_with("User spacewars may run the following commands on ") && s.ends_with(':')
        })
        .ok_or_else(|| invalid("unsupported effective sudo listing"))?;
    for line in &lines[..header] {
        if !(line.starts_with("Matching Defaults entries for spacewars on ") && line.ends_with(':'))
            && *line != "Runas and Command-specific defaults for spacewars:"
            && *line
                != r"secure_path=/usr/local/sbin\:/usr/local/bin\:/usr/sbin\:/usr/bin\:/sbin\:/bin"
            && *line != "Defaults!/usr/sbin/visudo env_keep+=\"SUDO_EDITOR EDITOR VISUAL\""
        {
            return Err(invalid("unreviewed effective sudo defaults"));
        }
    }
    let mut index = header + 1;
    let mut commands = BTreeSet::new();
    while index < lines.len() {
        let source = lines[index];
        if !matches!(
            source,
            "Sudoers entry: /etc/sudoers.d/spacewars-logs"
                | "Sudoers entry: /etc/sudoers.d/spacewars-renderer"
        ) || lines.get(index + 1) != Some(&"RunAsUsers: root")
            || lines.get(index + 2) != Some(&"Options: !authenticate")
            || lines.get(index + 3) != Some(&"Commands:")
        {
            return Err(invalid("unreviewed runtime sudo privilege"));
        }
        index += 4;
        let start = index;
        while index < lines.len() && !lines[index].starts_with("Sudoers entry:") {
            let command = lines[index];
            let allowed = if source.ends_with("spacewars-logs") {
                command == "/usr/sbin/spacewars-logs *"
            } else {
                matches!(
                    command,
                    "/usr/sbin/spacewars-renderer software" | "/usr/sbin/spacewars-renderer gpu"
                )
            };
            if !allowed || !commands.insert(command) {
                return Err(invalid("unreviewed or duplicate runtime sudo command"));
            }
            index += 1;
        }
        if start == index {
            return Err(invalid("empty effective sudo entry"));
        }
    }
    if !commands.contains("/usr/sbin/spacewars-logs *")
        || commands.contains("/usr/sbin/spacewars-renderer gpu")
            != commands.contains("/usr/sbin/spacewars-renderer software")
    {
        return Err(invalid("incomplete expected runtime sudo policy"));
    }
    Ok(commands.len())
}

fn ssh_configuration(output: &str) -> Result<()> {
    let mut fields = BTreeMap::new();
    let mut hostkeys = BTreeSet::new();
    for line in output.lines() {
        let (key, value) = line
            .split_once(' ')
            .ok_or_else(|| invalid("invalid effective SSH configuration"))?;
        if key == "hostkey" {
            if !hostkeys.insert(value) {
                return Err(invalid("duplicate SSH host key path"));
            }
        } else if matches!(
            key,
            "authorizedkeysfile"
                | "authorizedkeyscommand"
                | "authorizedprincipalsfile"
                | "authorizedprincipalscommand"
                | "trustedusercakeys"
                | "pubkeyauthentication"
                | "strictmodes"
                | "usedns"
                | "forcecommand"
                | "chrootdirectory"
                | "hostbasedauthentication"
                | "permitrootlogin"
                | "authenticationmethods"
                | "allowusers"
                | "denyusers"
                | "allowgroups"
                | "denygroups"
        ) && fields.insert(key, value).is_some()
        {
            return Err(invalid("duplicate effective SSH field"));
        }
    }
    for (key, expected) in [
        ("authorizedkeysfile", "/data/spacewars-access/%u"),
        ("authorizedkeyscommand", "none"),
        ("authorizedprincipalsfile", "none"),
        ("authorizedprincipalscommand", "none"),
        ("trustedusercakeys", "none"),
        ("pubkeyauthentication", "yes"),
        ("strictmodes", "yes"),
        ("usedns", "no"),
        ("forcecommand", "none"),
        ("chrootdirectory", "none"),
        ("hostbasedauthentication", "no"),
    ] {
        if fields.get(key) != Some(&expected) {
            return Err(invalid(
                "effective SSH configuration does not use the preserved key roles",
            ));
        }
    }
    if !matches!(
        fields.get("permitrootlogin"),
        Some(&"prohibit-password" | &"without-password")
    ) || !matches!(
        fields.get("authenticationmethods"),
        Some(&"any" | &"publickey")
    ) || ["allowusers", "denyusers", "allowgroups", "denygroups"]
        .iter()
        .any(|key| fields.contains_key(key))
        || hostkeys
            != BTreeSet::from([
                "/data/ssh/ssh_host_rsa_key",
                "/data/ssh/ssh_host_ecdsa_key",
                "/data/ssh/ssh_host_ed25519_key",
            ])
    {
        return Err(invalid(
            "unsupported SSH authentication or host-identity configuration",
        ));
    }
    Ok(())
}

fn ssh_environment(contents: &str) -> Result<()> {
    let mut assigned = false;
    for line in contents
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
    {
        if assigned || !matches!(line, "SSHD_OPTS=" | "SSHD_OPTS=\"\"" | "SSHD_OPTS=''") {
            return Err(invalid("unreviewed SSH command-line environment override"));
        }
        assigned = true;
    }
    Ok(())
}

const PROPERTIES: &str = "LoadState,NeedDaemonReload,User,Group,DynamicUser,RootDirectory,RootImage,BindPaths,BindReadOnlyPaths,TemporaryFileSystem,ExecStart,ExecStartPre,ExecStartPost,ExecCondition,Environment,EnvironmentFiles,PassEnvironment,UnsetEnvironment,Requires,Wants,After";
type Service = BTreeMap<String, String>;

fn service(run: &mut impl FnMut(&str, &[&str]) -> Result<Vec<u8>>, unit: &str) -> Result<Service> {
    let bytes = run(
        "/usr/bin/systemctl",
        &[
            "show",
            unit,
            "--no-pager",
            &format!("--property={PROPERTIES}"),
        ],
    )?;
    service_fields(text(&bytes)?)
}

fn service_fields(output: &str) -> Result<Service> {
    let mut fields = BTreeMap::new();
    for line in output.lines() {
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| invalid("invalid loaded service properties"))?;
        if fields.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(invalid("duplicate loaded service property"));
        }
    }
    Ok(fields)
}

fn value<'a>(fields: &'a Service, key: &str) -> &'a str {
    fields.get(key).map(String::as_str).unwrap_or("")
}

fn service_base(fields: &Service) -> Result<()> {
    if value(fields, "LoadState") != "loaded"
        || value(fields, "NeedDaemonReload") != "no"
        || value(fields, "DynamicUser") != "no"
        || !matches!(value(fields, "User"), "" | "root")
        || !matches!(value(fields, "Group"), "" | "root")
        || [
            "RootDirectory",
            "RootImage",
            "BindPaths",
            "BindReadOnlyPaths",
            "TemporaryFileSystem",
            "ExecStartPost",
            "ExecCondition",
        ]
        .iter()
        .any(|key| !value(fields, key).is_empty())
    {
        return Err(invalid(
            "service is stale, unavailable or uses unsupported overrides",
        ));
    }
    Ok(())
}

fn needs(fields: &Service, property: &str, unit: &str) -> Result<()> {
    if !value(fields, property)
        .split_ascii_whitespace()
        .any(|s| s == unit)
    {
        return Err(invalid("missing loaded service dependency"));
    }
    Ok(())
}

fn executable(fields: &Service, property: &str, path: &str, arguments: &str) -> Result<()> {
    let value = value(fields, property);
    if value.matches('{').count() != 1
        || value.matches('}').count() != 1
        || !value.starts_with(&format!(
            "{{ path={path} ; argv[]={path}{arguments} ; ignore_errors="
        ))
    {
        return Err(invalid("unreviewed loaded service command"));
    }
    Ok(())
}

fn ssh_service(fields: &Service, mode: &str) -> Result<()> {
    service_base(fields)?;
    needs(fields, "Wants", "spacewars-access.service")?;
    needs(fields, "After", "spacewars-access.service")?;
    executable(
        fields,
        "ExecStart",
        "/usr/sbin/sshd",
        &format!(" {mode} $SSHD_OPTS"),
    )?;
    if !value(fields, "ExecStartPre").is_empty()
        || !value(fields, "PassEnvironment").is_empty()
        || !value(fields, "UnsetEnvironment").is_empty()
        || !matches!(value(fields, "Environment"), "" | "SSHD_OPTS=")
        || !matches!(
            value(fields, "EnvironmentFiles"),
            "" | "/etc/default/ssh (ignore_errors=yes)"
        )
    {
        return Err(invalid("unreviewed SSH service override"));
    }
    Ok(())
}

fn persistence_service(fields: &Service, unit: &str) -> Result<()> {
    service_base(fields)?;
    match unit {
        "persistent-data-init.service" => {
            executable(fields, "ExecStart", "/usr/sbin/persistent-data-init", "")?;
            needs(fields, "Requires", "data.mount")?;
            needs(fields, "After", "data.mount")?;
        }
        "persistent-data-bind.service" => {
            executable(
                fields,
                "ExecStartPre",
                "/usr/bin/engine-os-manager",
                " initialize-network",
            )?;
            executable(
                fields,
                "ExecStart",
                "/bin/mount",
                " --bind /data/NetworkManager/system-connections /etc/NetworkManager/system-connections",
            )?;
            needs(fields, "Requires", "persistent-data-init.service")?;
            needs(fields, "Requires", "data.mount")?;
            needs(fields, "After", "persistent-data-init.service")?;
            needs(fields, "After", "data.mount")?;
        }
        "spacewars-data-init.service" => {
            executable(fields, "ExecStart", "/usr/bin/spacewars-data-init", "")?;
            needs(fields, "Requires", "data.mount")?;
            needs(fields, "Requires", "persistent-data-init.service")?;
            needs(fields, "After", "data.mount")?;
            needs(fields, "After", "persistent-data-init.service")?;
        }
        "spacewars-access.service" => {
            executable(
                fields,
                "ExecStart",
                "/usr/bin/engine-os-manager",
                " initialize-access",
            )?;
            needs(fields, "Requires", "spacewars-data-init.service")?;
            needs(fields, "Wants", "persistent-data-bind.service")?;
            needs(fields, "After", "spacewars-data-init.service")?;
            needs(fields, "After", "persistent-data-bind.service")?;
        }
        "NetworkManager.service" => {
            needs(fields, "Requires", "persistent-data-bind.service")?;
            needs(fields, "After", "persistent-data-bind.service")?;
        }
        _ => return Err(invalid("unsupported persistence service")),
    }
    Ok(())
}

#[cfg(test)]
#[path = "bootstrap_access_check_tests.rs"]
mod tests;
