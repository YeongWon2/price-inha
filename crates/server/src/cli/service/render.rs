//! 서비스 정의 파일의 내용(plist · unit · 작업 XML)과 그 글자 다루기.

use super::{CONFIG_FATAL_EXIT, LAUNCHD_LABEL, SERVER_ERROR_LOG, SERVER_LOG, Service};

impl Service<'_> {
    /// macOS 사용자 에이전트 정의.
    pub(super) fn plist(&self) -> String {
        let args: String = std::iter::once(self.setup.exe.to_string_lossy().into_owned())
            .chain(self.program_args())
            .map(|arg| format!("    <string>{}</string>\n", xml_escape(&arg)))
            .collect();
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{label}</string>
  <key>ProgramArguments</key>
  <array>
{args}  </array>
  <key>WorkingDirectory</key>
  <string>{home}</string>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PATH</key>
    <string>{path}</string>
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
  <key>ThrottleInterval</key>
  <integer>30</integer>
  <key>ExitTimeOut</key>
  <integer>30</integer>
  <key>Umask</key>
  <integer>63</integer>
  <key>ProcessType</key>
  <string>Background</string>
  <key>StandardOutPath</key>
  <string>{out}</string>
  <key>StandardErrorPath</key>
  <string>{err}</string>
</dict>
</plist>
"#,
            label = LAUNCHD_LABEL,
            home = xml_escape(&self.home_text()),
            path = xml_escape(&self.setup.path_env),
            out = xml_escape(&self.log_path(SERVER_LOG)),
            err = xml_escape(&self.log_path(SERVER_ERROR_LOG)),
        )
    }

    /// Linux 사용자 unit.
    pub(super) fn unit(&self) -> String {
        let exec: Vec<String> = std::iter::once(self.setup.exe.to_string_lossy().into_owned())
            .chain(self.program_args())
            .map(|arg| systemd_quote(&arg))
            .collect();
        format!(
            "[Unit]
Description=price-inha server
After=network-online.target
Wants=network-online.target
StartLimitIntervalSec=300
StartLimitBurst=10

[Service]
Type=simple
ExecStart={exec}
WorkingDirectory={home}
Environment={path}
Restart=on-failure
RestartSec=5
RestartPreventExitStatus={fatal}
KillSignal=SIGTERM
TimeoutStopSec=30
UMask=0077
NoNewPrivileges=yes

[Install]
WantedBy=default.target
",
            exec = exec.join(" "),
            home = self.home_text(),
            path = systemd_quote(&format!("PATH={}", self.setup.path_env)),
            fatal = CONFIG_FATAL_EXIT,
        )
    }

    /// Windows 예약 작업 XML.
    pub(super) fn task_xml(&self) -> String {
        let arguments: Vec<String> = self
            .program_args()
            .iter()
            .map(|arg| windows_quote(arg))
            .collect();
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>price-inha server</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <Delay>PT30S</Delay>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <RestartOnFailure>
      <Interval>PT1M</Interval>
      <Count>999</Count>
    </RestartOnFailure>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{exe}</Command>
      <Arguments>{arguments}</Arguments>
      <WorkingDirectory>{home}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
"#,
            exe = xml_escape(&self.setup.exe.to_string_lossy()),
            arguments = xml_escape(&arguments.join(" ")),
            home = xml_escape(&self.home_text()),
        )
    }
}

pub(super) fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(super) fn xml_unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// `start`와 `end` 사이 글(처음 것).
pub(super) fn between<'t>(text: &'t str, start: &str, end: &str) -> Option<&'t str> {
    let from = text.find(start)? + start.len();
    let to = text[from..].find(end)? + from;
    Some(&text[from..to])
}

/// plist에서 `WorkingDirectory` 다음 `<string>`의 값.
pub(super) fn plist_working_dir(plist: &str) -> Option<String> {
    let after = &plist[plist.find("<key>WorkingDirectory</key>")?..];
    between(after, "<string>", "</string>").map(xml_unescape)
}

/// systemd 인자 하나. 빈칸 · 따옴표 · 역빗금이 있으면 큰따옴표로 감싼다. `%`는 `%%`로.
pub(super) fn systemd_quote(arg: &str) -> String {
    let arg = arg.replace('%', "%%");
    if arg.is_empty()
        || arg
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '"' | '\\' | '\''))
    {
        format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        arg
    }
}

/// Windows 명령줄 인자 하나. 빈칸이 있으면 큰따옴표로 감싼다.
pub(super) fn windows_quote(arg: &str) -> String {
    if arg.is_empty() || arg.chars().any(|c| c.is_whitespace() || c == '"') {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 따옴표는_필요할_때만() {
        assert_eq!(systemd_quote("/a/b"), "/a/b");
        assert_eq!(systemd_quote("/a b/c"), "\"/a b/c\"");
        assert_eq!(systemd_quote("50%"), "50%%");
        assert_eq!(windows_quote("C:\\a b"), "\"C:\\a b\"");
    }

    #[test]
    fn xml_글자를_되돌린다() {
        let text = "/a&b/<c>";
        assert_eq!(xml_unescape(&xml_escape(text)), text);
    }
}
