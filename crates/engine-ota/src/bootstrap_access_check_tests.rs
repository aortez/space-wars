use super::*;

const SUDO: &str = r#"Matching Defaults entries for spacewars on fixture:
    secure_path=/usr/local/sbin\:/usr/local/bin\:/usr/sbin\:/usr/bin\:/sbin\:/bin
Runas and Command-specific defaults for spacewars:
    Defaults!/usr/sbin/visudo env_keep+="SUDO_EDITOR EDITOR VISUAL"
User spacewars may run the following commands on fixture:
Sudoers entry: /etc/sudoers.d/spacewars-logs
    RunAsUsers: root
    Options: !authenticate
    Commands:
        /usr/sbin/spacewars-logs *
Sudoers entry: /etc/sudoers.d/spacewars-renderer
    RunAsUsers: root
    Options: !authenticate
    Commands:
        /usr/sbin/spacewars-renderer software
        /usr/sbin/spacewars-renderer gpu
"#;

const SSH: &str = "authorizedkeysfile /data/spacewars-access/%u\nauthorizedkeyscommand none\nauthorizedprincipalsfile none\nauthorizedprincipalscommand none\ntrustedusercakeys none\npubkeyauthentication yes\nstrictmodes yes\nusedns no\nforcecommand none\nchrootdirectory none\nhostbasedauthentication no\npermitrootlogin without-password\nauthenticationmethods any\nhostkey /data/ssh/ssh_host_rsa_key\nhostkey /data/ssh/ssh_host_ecdsa_key\nhostkey /data/ssh/ssh_host_ed25519_key\n";

#[test]
fn administrator_requires_separate_image_account_and_exact_passwordless_root_grant() {
    let policy = SUDO
        .split("User spacewars may")
        .next()
        .unwrap()
        .replace("spacewars", "swadmin")
        + "User swadmin may run the following commands on fixture:\nSudoers entry: /etc/sudoers.d/spacewars-admin\nRunAsUsers: root\nOptions: !authenticate\nCommands:\nALL\n";
    administrator_sudo_policy(&policy).unwrap();
    assert!(sudo_policy(&policy.replace("swadmin", "spacewars")).is_err());
    for (from, to) in [
        ("RunAsUsers: root", "RunAsUsers: ALL"),
        ("!authenticate", "authenticate"),
        ("Commands:\nALL", "Commands:\n/usr/bin/id"),
        ("spacewars-admin", "spacewars-logs"),
        ("!authenticate", "!authenticate, setenv"),
    ] {
        assert!(
            administrator_sudo_policy(&policy.replace(from, to)).is_err(),
            "{to}"
        );
    }
    assert!(administrator_sudo_policy(&format!("{policy}ALL\n")).is_err());
    let account = "swadmin:x:1001:1001:Owner:/home/swadmin:/bin/bash";
    assert_eq!(
        administrator_account(&format!("root:x:0:0::/root:/bin/sh\n{account}\n")).unwrap(),
        account
    );
    for (from, to) in [
        ("1001", "1000"),
        (":x:", ":!:"),
        ("/bin/bash", "/bin/false"),
        ("/home/swadmin", "/home/spacewars"),
    ] {
        assert!(
            administrator_account(&account.replace(from, to)).is_err(),
            "{to}"
        );
    }
    assert!(administrator_account(&format!("{account}\n{account}\n")).is_err());
    assert!(administrator_account("").is_err());
}

#[test]
fn effective_sudo_listing_rejects_old_installers_broad_grants_environment_and_runas_changes() {
    assert_eq!(sudo_policy(SUDO).unwrap(), 3);
    let logs_only = SUDO
        .split("Sudoers entry: /etc/sudoers.d/spacewars-renderer")
        .next()
        .unwrap();
    assert_eq!(sudo_policy(logs_only).unwrap(), 1);
    for (from, to) in [
        (
            "/usr/sbin/spacewars-renderer gpu",
            "/usr/sbin/spacewars-fast-update *",
        ),
        (
            "/usr/sbin/spacewars-renderer gpu",
            "/usr/sbin/ab-update-with-key *",
        ),
        (
            "/usr/sbin/spacewars-renderer gpu",
            "/usr/bin/systemctl reboot",
        ),
        ("/usr/sbin/spacewars-renderer gpu", "ALL"),
        (
            "/usr/sbin/spacewars-renderer gpu",
            "/usr/sbin/spacewars-renderer *",
        ),
        ("RunAsUsers: root", "RunAsUsers: ALL"),
        ("Options: !authenticate", "Options: !authenticate, setenv"),
        (
            "Options: !authenticate",
            "RunAsGroups: ALL\nOptions: !authenticate",
        ),
        ("secure_path=", "!env_reset, secure_path="),
        ("Defaults!/usr/sbin/visudo", "Defaults"),
        ("spacewars-renderer\n", "spacewars-ota\n"),
    ] {
        assert!(sudo_policy(&SUDO.replace(from, to)).is_err(), "{to}");
    }
    assert!(sudo_policy(&format!("{SUDO}\t/usr/sbin/spacewars-renderer gpu\n")).is_err());
}

#[test]
fn ssh_requires_persistent_keys_host_identity_and_unambiguous_public_key_authentication() {
    ssh_configuration(SSH).unwrap();
    // The installed Pi emits a separate listenaddress for IPv4 and IPv6.
    ssh_configuration(&format!(
        "{SSH}listenaddress [::]:22\nlistenaddress 0.0.0.0:22\n"
    ))
    .unwrap();
    for (from, to) in [
        ("/data/spacewars-access/%u", ".ssh/authorized_keys"),
        (
            "authorizedkeyscommand none",
            "authorizedkeyscommand /usr/bin/other-keys",
        ),
        ("trustedusercakeys none", "trustedusercakeys /etc/ssh/ca"),
        ("permitrootlogin without-password", "permitrootlogin no"),
        ("permitrootlogin without-password", "permitrootlogin yes"),
        ("pubkeyauthentication yes", "pubkeyauthentication no"),
        ("strictmodes yes", "strictmodes no"),
        ("usedns no", "usedns yes"),
        (
            "authenticationmethods any",
            "authenticationmethods publickey,password",
        ),
        ("forcecommand none", "forcecommand internal-sftp"),
        (
            "/data/ssh/ssh_host_ed25519_key",
            "/etc/ssh/ssh_host_ed25519_key",
        ),
    ] {
        assert!(ssh_configuration(&SSH.replace(from, to)).is_err(), "{to}");
    }
    assert!(ssh_configuration(&format!("{SSH}denyusers root\n")).is_err());
    assert!(
        ssh_configuration(&format!(
            "{SSH}authorizedkeysfile /data/spacewars-access/%u\n"
        ))
        .is_err()
    );
}

#[test]
fn ssh_connection_is_bounded_data_and_cannot_inject_more_match_fields() {
    let connection = SshCheckConnection::parse("192.0.2.10 34567 192.0.2.20 22").unwrap();
    assert_eq!(
        connection.argument("root"),
        "user=root,addr=192.0.2.10,host=192.0.2.10,laddr=192.0.2.20,lport=22"
    );
    SshCheckConnection::parse("2001:db8::1 50000 2001:db8::2 22").unwrap();
    for bad in [
        "",
        "192.0.2.1,user=root 1 192.0.2.2 22",
        "localhost 1 192.0.2.2 22",
        "192.0.2.1 0 192.0.2.2 22",
        "192.0.2.1 1 192.0.2.2 65536",
        "192.0.2.1 1 192.0.2.2 22 user=root",
    ] {
        assert!(SshCheckConnection::parse(bad).is_err());
    }
}

#[test]
fn ssh_environment_cannot_override_the_configuration_being_checked() {
    for text in [
        "",
        "# defaults\nSSHD_OPTS=\"\"\n",
        "SSHD_OPTS=''",
        "SSHD_OPTS=",
    ] {
        ssh_environment(text).unwrap();
    }
    for text in [
        "SSHD_OPTS=-oAuthorizedKeysFile=/etc/keys",
        "SSHD_OPTS='-f /tmp/sshd_config'",
        "SSHD_OPTS=\nSSHD_OPTS=",
        "export SSHD_OPTS=",
        "LD_PRELOAD=/tmp/library.so",
        "SSHD_OPTS=$(whoami)",
    ] {
        assert!(ssh_environment(text).is_err());
    }
}

fn unit() -> Service {
    [("LoadState", "loaded"), ("NeedDaemonReload", "no"), ("DynamicUser", "no"),
        ("Wants", "spacewars-access.service"), ("After", "basic.target spacewars-access.service"),
        ("Environment", "SSHD_OPTS="), ("EnvironmentFiles", "/etc/default/ssh (ignore_errors=yes)"),
        ("ExecStart", "{ path=/usr/sbin/sshd ; argv[]=/usr/sbin/sshd -i $SSHD_OPTS ; ignore_errors=yes ; pid=0 }")]
        .into_iter().map(|(k,v)| (k.into(),v.into())).collect()
}

#[test]
fn loaded_service_checks_reject_stale_configuration_redirects_and_alternate_ssh_arguments() {
    ssh_service(&unit(), "-i").unwrap();
    for (key, value) in [
        ("NeedDaemonReload", "yes"),
        ("User", "spacewars"),
        ("DynamicUser", "yes"),
        ("RootDirectory", "/other"),
        ("BindPaths", "/tmp:/etc/ssh"),
        ("Wants", ""),
        ("After", "basic.target"),
        ("Environment", "SSHD_OPTS=-f /other"),
        ("EnvironmentFiles", "/etc/other (ignore_errors=yes)"),
        ("PassEnvironment", "SSHD_OPTS"),
        ("UnsetEnvironment", "SSHD_OPTS"),
        (
            "ExecStart",
            "{ path=/usr/sbin/sshd ; argv[]=/usr/sbin/sshd -i -f /other ; ignore_errors=yes }",
        ),
        (
            "ExecStart",
            "{ path=/usr/sbin/sshd ; argv[]=/usr/sbin/sshd -i $SSHD_OPTS ; ignore_errors=yes } { path=/bin/sh ; argv[]=/bin/sh ; ignore_errors=no }",
        ),
    ] {
        let mut fields = unit();
        fields.insert(key.into(), value.into());
        assert!(ssh_service(&fields, "-i").is_err(), "{key}");
    }
}

#[test]
fn ssh_admission_hook_must_be_exact_and_cannot_ignore_a_failure() {
    let hook = "{ path=/usr/bin/engine-os-manager ; argv[]=/usr/bin/engine-os-manager ssh-network-admit ; ignore_errors=no ; pid=0 }";
    let mut fields = unit();
    fields.insert("ExecStartPre".into(), hook.into());
    ssh_service(&fields, "-i").unwrap();
    for value in [
        hook.replace("ignore_errors=no", "ignore_errors=yes"),
        hook.replace("ssh-network-admit ;", "ssh-network-admit --skip ;"),
        format!("{hook} {hook}"),
        "{ path=/bin/true ; argv[]=/bin/true ; ignore_errors=no ; pid=0 }".into(),
    ] {
        fields.insert("ExecStartPre".into(), value);
        assert!(ssh_service(&fields, "-i").is_err());
    }
    fields = unit();
    fields.insert(
        "ExecStart".into(),
        fields["ExecStart"].replace(" -i ", " -D "),
    );
    fields.insert("ExecStartPre".into(), hook.into());
    assert!(ssh_service(&fields, "-D").is_err());
}

#[test]
fn configuration_parser_never_returns_unexpected_input_bytes_as_diagnostics() {
    let marker = "DO-NOT-LOG-THIS";
    for error in [
        sudo_policy(marker).unwrap_err(),
        ssh_configuration(marker).unwrap_err(),
        ssh_environment(marker).unwrap_err(),
    ] {
        assert!(!error.to_string().contains(marker));
    }
    for bytes in [b"invalid\0configuration".as_slice(), b"bad\x1b[2J"] {
        assert!(text(bytes).is_err());
    }
}

#[test]
fn pi5_observations_are_parseable_but_unmigrated_ssh_dependencies_are_rejected() {
    // Captured using sshd -G against a temporary config. No live configuration
    // changed, and this does not establish successful logins or host-key access.
    let config = include_str!("../tests/fixtures/pi5-sshd-access-overlay.txt");
    ssh_configuration(config).unwrap();
    assert!(
        ssh_configuration(&config.replace("/data/spacewars-access/%u", ".ssh/authorized_keys"))
            .is_err()
    );

    let before = include_str!("../tests/fixtures/pi5-sshd-service-before.txt");
    let mut fields = service_fields(before).unwrap();
    assert!(ssh_service(&fields, "-i").is_err());
    fields.insert("Wants".into(), "spacewars-access.service".into());
    fields
        .get_mut("After")
        .unwrap()
        .push_str(" spacewars-access.service");
    ssh_service(&fields, "-i").unwrap();
    assert!(service_fields(&format!("{before}LoadState=loaded\n")).is_err());
}

#[test]
fn network_and_access_dependencies_require_successful_initialization_and_ordering() {
    let mut fields = Service::from([
        ("LoadState".into(), "loaded".into()),
        ("NeedDaemonReload".into(), "no".into()),
        ("DynamicUser".into(), "no".into()),
        (
            "Requires".into(),
            "sysinit.target persistent-data-bind.service".into(),
        ),
        (
            "After".into(),
            "basic.target persistent-data-bind.service".into(),
        ),
    ]);
    persistence_service(&fields, "NetworkManager.service").unwrap();
    fields.insert("Requires".into(), "sysinit.target".into());
    // The installed legacy unit has After without Requires; it must not pass.
    assert!(persistence_service(&fields, "NetworkManager.service").is_err());

    fields.insert("ExecStart".into(), "{ path=/usr/bin/engine-os-manager ; argv[]=/usr/bin/engine-os-manager initialize-access ; ignore_errors=no }".into());
    fields.insert("Requires".into(), "spacewars-data-init.service".into());
    fields.insert("Wants".into(), "persistent-data-bind.service".into());
    fields.insert(
        "After".into(),
        "spacewars-data-init.service persistent-data-bind.service".into(),
    );
    persistence_service(&fields, "spacewars-access.service").unwrap();
    for (key, changed) in [
        ("Requires", ""),
        ("Wants", ""),
        ("After", "persistent-data-bind.service"),
        ("After", "spacewars-data-init.service"),
    ] {
        let mut missing = fields.clone();
        missing.insert(key.into(), changed.into());
        assert!(
            persistence_service(&missing, "spacewars-access.service").is_err(),
            "{key}={changed}"
        );
    }
}

#[test]
fn reviewed_log_helper_digest_matches_its_repository_source() {
    let bytes = include_bytes!(
        "../../../yocto/meta-spacewars/recipes-spacewars/spacewars/files/spacewars-logs.sh"
    );
    assert_eq!(Sha256::digest(bytes).to_string(), LOG_HELPER_SHA256);
}
