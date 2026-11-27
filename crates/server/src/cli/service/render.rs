//! 서비스 정의 파일의 내용(plist · unit · 작업 XML)과 그 글자 다루기.

use super::{
    CONFIG_FATAL_EXIT, LAUNCHD_LABEL, SERVER_ERROR_LOG, SERVER_LOG, SERVER_LOGS_DIR, Service,
};

/// `run`의 기록 파일 인자(Windows 예약 작업 정의가 싣는다).
const LOG_FILE_ARG: &str = "--log-file";

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
            .map(|arg| systemd_exec_quote(&arg))
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
            home = systemd_specifiers(&self.home_text()),
            path = systemd_quote(&format!("PATH={}", self.setup.path_env)),
            fatal = CONFIG_FATAL_EXIT,
        )
    }

    /// Windows 예약 작업 XML.
    pub(super) fn task_xml(&self) -> String {
        let arguments: Vec<String> = self
            .windows_args()
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

impl Service<'_> {
    /// Windows 예약 작업의 인자: `run` 인자에 기록 파일(`<home>\logs\server.log`)을 더한다. 예약 작업은
    /// 표준 출력을 파일로 돌리지 못한다(9차 보강 3). 사용자가 `--log-file`을 이미 줬으면 그것을 따른다.
    fn windows_args(&self) -> Vec<String> {
        let mut args = self.program_args();
        if !args.iter().any(|arg| arg == LOG_FILE_ARG) {
            args.push(LOG_FILE_ARG.to_string());
            args.push(windows_join(
                &self.home_text(),
                &[SERVER_LOGS_DIR, SERVER_LOG],
            ));
        }
        args
    }
}

/// Windows 경로를 `\`로 잇는다(정의는 다른 운영체제에서도 같은 글자로 만들어져야 한다).
fn windows_join(base: &str, parts: &[&str]) -> String {
    let mut path = base.to_string();
    for part in parts {
        if !path.ends_with(['\\', '/']) {
            path.push('\\');
        }
        path.push_str(part);
    }
    path
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

/// (65차) systemd `ExecStart` 인자 하나: [`systemd_quote`]에 더해 `$`를 `$$`로 적는다(환경 변수 풀이를 막는다).
pub(super) fn systemd_exec_quote(arg: &str) -> String {
    systemd_quote(&arg.replace('$', "$$"))
}

/// (65차) systemd 지정자 풀이를 막는다: `%` → `%%`(`WorkingDirectory=`는 따옴표 · `$` 풀이가 없다).
pub(super) fn systemd_specifiers(text: &str) -> String {
    text.replace('%', "%%")
}

/// (65차) unit에서 읽은 `WorkingDirectory=` 값을 되돌린다(`%%` → `%`).
pub(super) fn systemd_unspecifiers(text: &str) -> String {
    text.replace("%%", "%")
}

/// Windows 명령줄 인자 하나. 빈칸 · 따옴표가 있으면 큰따옴표로 감싼다(`CommandLineToArgvW` 규칙). (65차)
/// 따옴표 앞의 `\`와 끝의 `\`는 겹쳐 적어 닫는 따옴표가 글자로 읽히지 않게 한다.
pub(super) fn windows_quote(arg: &str) -> String {
    if !(arg.is_empty() || arg.chars().any(|c| c.is_whitespace() || c == '"')) {
        return arg.to_string();
    }
    let mut quoted = String::from('"');
    let mut slashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                quoted.push_str(&"\\".repeat(slashes * 2 + 1));
                quoted.push('"');
                slashes = 0;
            }
            other => {
                quoted.push_str(&"\\".repeat(slashes));
                quoted.push(other);
                slashes = 0;
            }
        }
    }
    quoted.push_str(&"\\".repeat(slashes * 2));
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_경로는_역슬래시로_잇는다() {
        assert_eq!(
            windows_join(r"C:\Users\me\.price-inha", &["logs", "server.log"]),
            r"C:\Users\me\.price-inha\logs\server.log"
        );
        assert_eq!(windows_join(r"C:\", &["logs"]), r"C:\logs");
    }

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
