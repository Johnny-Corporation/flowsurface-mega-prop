use std::{
    backtrace::Backtrace,
    fs,
    io::{self, BufRead, Write},
    panic::PanicHookInfo,
    path::{Path, PathBuf},
    sync::{Once, OnceLock, mpsc},
    thread::{self, JoinHandle},
};

use regex::Regex;

const MAX_LOG_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB
const LOG_EXPORT_DIR: &str = "exports";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LogExportMode {
    All,
    Sections,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LogExport {
    pub(crate) path: PathBuf,
    pub(crate) files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogSection {
    App,
    Ui,
    Connection,
    Exchange,
    Trading,
}

impl LogSection {
    const ALL: [Self; 5] = [
        Self::App,
        Self::Ui,
        Self::Connection,
        Self::Exchange,
        Self::Trading,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::App => "app",
            Self::Ui => "ui",
            Self::Connection => "connection",
            Self::Exchange => "exchange",
            Self::Trading => "trading",
        }
    }

    const fn file_name(self) -> &'static str {
        match self {
            Self::App => "app.log",
            Self::Ui => "ui.log",
            Self::Connection => "connection.log",
            Self::Exchange => "exchange.log",
            Self::Trading => "trading.log",
        }
    }
}

enum LogMessage {
    Content(Vec<u8>),
    Flush,
    Shutdown,
}

pub fn setup(is_debug: bool) -> Result<(), data::log::Error> {
    let default_level = if is_debug {
        log::Level::Debug
    } else {
        log::Level::Info
    };

    let level_filter = std::env::var("RUST_LOG")
        .ok()
        .as_deref()
        .map(str::parse::<log::Level>)
        .transpose()?
        .unwrap_or(default_level)
        .to_level_filter();

    let mut io_sink = fern::Dispatch::new().format(|out, message, record| {
        let message = message.to_string();
        let section = log_section_for_record(record.target(), &message);
        let message = redact_log_line(&message);

        out.finish(format_args!(
            "{}:{} [{}] -- {}",
            chrono::Local::now().format("%H:%M:%S%.3f"),
            record.level(),
            section.label(),
            message
        ));
    });

    if is_debug {
        io_sink = io_sink.chain(std::io::stdout());
    } else {
        let log_path = data::log::path()?;
        initial_rotation(&log_path)?;

        let logger: Box<dyn Write + Send> = Box::new(BackgroundLogger::new(log_path)?);

        io_sink = io_sink.chain(logger);
    }

    fern::Dispatch::new()
        .level(log::LevelFilter::Off)
        .level_for("panic", log::LevelFilter::Error)
        .level_for("iced_wgpu", log::LevelFilter::Info)
        .level_for("flowsurface_exchange", level_filter)
        .level_for("flowsurface_data", level_filter)
        .level_for("flowsurface", level_filter)
        .chain(io_sink)
        .apply()?;

    Ok(())
}

pub(crate) fn export_all_logs() -> io::Result<LogExport> {
    let output_root = log_export_root()?;
    let sources = existing_runtime_log_paths()?;

    export_logs_from_paths(&sources, &output_root, LogExportMode::All)
}

pub(crate) fn export_section_logs() -> io::Result<LogExport> {
    let output_root = log_export_root()?;
    let sources = existing_runtime_log_paths()?;

    export_logs_from_paths(&sources, &output_root, LogExportMode::Sections)
}

fn existing_runtime_log_paths() -> io::Result<Vec<PathBuf>> {
    let current = data::log::path().map_err(|err| io::Error::other(err.to_string()))?;
    let dir = current.parent().unwrap_or_else(|| Path::new("."));
    let previous = dir.join("flowsurface-previous.log");

    Ok([current, previous]
        .into_iter()
        .filter(|path| path.exists())
        .collect())
}

fn log_export_root() -> io::Result<PathBuf> {
    let current = data::log::path().map_err(|err| io::Error::other(err.to_string()))?;
    let dir = current.parent().unwrap_or_else(|| Path::new("."));
    Ok(dir.join(LOG_EXPORT_DIR))
}

fn export_logs_from_paths(
    source_paths: &[PathBuf],
    output_root: &Path,
    mode: LogExportMode,
) -> io::Result<LogExport> {
    let export_path = output_root.join(format!(
        "flowsurface-logs-{}-{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S"),
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&export_path)?;

    match mode {
        LogExportMode::All => export_all_log_file(source_paths, &export_path),
        LogExportMode::Sections => export_section_log_files(source_paths, &export_path),
    }
}

fn export_all_log_file(source_paths: &[PathBuf], export_path: &Path) -> io::Result<LogExport> {
    let file_path = export_path.join("all.log");
    let mut file = fs::File::create(&file_path)?;

    for source in source_paths {
        write_redacted_log_source(source, &mut file)?;
    }

    Ok(LogExport {
        path: export_path.to_path_buf(),
        files: vec![file_path],
    })
}

fn export_section_log_files(source_paths: &[PathBuf], export_path: &Path) -> io::Result<LogExport> {
    let mut files = Vec::new();
    let mut outputs = Vec::new();

    for section in LogSection::ALL {
        let file_path = export_path.join(section.file_name());
        let file = fs::File::create(&file_path)?;
        files.push(file_path);
        outputs.push((section, file));
    }

    for source in source_paths {
        write_sectioned_log_source(source, &mut outputs)?;
    }

    Ok(LogExport {
        path: export_path.to_path_buf(),
        files,
    })
}

fn write_redacted_log_source(source: &Path, output: &mut fs::File) -> io::Result<()> {
    if !source.exists() {
        return Ok(());
    }

    writeln!(
        output,
        "# source: {}",
        source
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown")
    )?;

    let input = io::BufReader::new(fs::File::open(source)?);
    for line in input.lines() {
        writeln!(output, "{}", redact_log_line(&line?))?;
    }

    Ok(())
}

fn write_sectioned_log_source(
    source: &Path,
    outputs: &mut [(LogSection, fs::File)],
) -> io::Result<()> {
    if !source.exists() {
        return Ok(());
    }

    let input = io::BufReader::new(fs::File::open(source)?);
    for line in input.lines() {
        let line = redact_log_line(&line?);
        let section = classify_log_line(&line);
        if let Some((_, file)) = outputs
            .iter_mut()
            .find(|(candidate, _)| *candidate == section)
        {
            writeln!(file, "{line}")?;
        }
    }

    Ok(())
}

fn log_section_for_record(target: &str, message: &str) -> LogSection {
    if message_has_trading_marker(message) {
        return LogSection::Trading;
    }

    let target = target.to_ascii_lowercase();
    if target.contains("panel_window::connections")
        || target.contains("connection")
        || target.contains("private_ws")
        || target.contains("proxy")
    {
        return LogSection::Connection;
    }
    if target.contains("flowsurface_exchange") || target.contains("adapter") {
        return LogSection::Exchange;
    }
    if target.contains("screen")
        || target.contains("dashboard")
        || target.contains("chart")
        || target.contains("widget")
        || target.contains("modal")
    {
        return LogSection::Ui;
    }

    LogSection::App
}

fn classify_log_line(line: &str) -> LogSection {
    let lower = line.to_ascii_lowercase();

    for section in LogSection::ALL {
        if lower.contains(&format!("[{}]", section.label())) {
            return section;
        }
    }
    if message_has_trading_marker(line) {
        return LogSection::Trading;
    }
    if lower.contains("private websocket")
        || lower.contains("connection")
        || lower.contains("connected")
        || lower.contains("disconnected")
        || lower.contains("credential")
        || lower.contains("proxy")
    {
        return LogSection::Connection;
    }
    if lower.contains("exchange")
        || lower.contains("mexc")
        || lower.contains("bybit")
        || lower.contains("binance")
        || lower.contains("okx")
        || lower.contains("hyperliquid")
        || lower.contains("http")
        || lower.contains("request")
        || lower.contains("response")
        || lower.contains("stream")
    {
        return LogSection::Exchange;
    }
    if lower.contains("dom")
        || lower.contains("pane")
        || lower.contains("layout")
        || lower.contains("chart")
        || lower.contains("render")
        || lower.contains("window")
    {
        return LogSection::Ui;
    }

    LogSection::App
}

fn message_has_trading_marker(message: &str) -> bool {
    message.contains("DOM_ORDER")
        || message.contains("DOM_CANCEL")
        || message.contains("DOM_LIVE_ORDER")
        || message.contains("MEXC_ORDER")
        || message.contains("MEXC_PRIVATE_ORDER")
        || message.contains("MEXC_PRIVATE_POSITION")
        || message.contains("MEXC_CANCEL")
        || message.contains("ORDER_REST")
}

fn redact_log_line(line: &str) -> String {
    if !contains_sensitive_marker(line) {
        return line.to_string();
    }

    let redacted = bearer_token_regex().replace_all(line, "Bearer [redacted]");
    let redacted = sensitive_pair_regex().replace_all(&redacted, "$1=[redacted]");
    let redacted = sensitive_json_regex().replace_all(&redacted, "$1\"[redacted]\"");
    redacted.into_owned()
}

fn contains_sensitive_marker(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("api")
        || lower.contains("secret")
        || lower.contains("signature")
        || lower.contains("token")
        || lower.contains("bearer")
        || lower.contains("authorization")
        || lower.contains("password")
        || lower.contains("passwd")
}

fn sensitive_pair_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r#"(?i)\b(api[_-]?key|access[_-]?key|secret[_-]?key|signature|sign|token|authorization|password|passwd)\b\s*[:=]\s*("[^"]*"|'[^']*'|[^,\s}]+)"#,
        )
        .expect("sensitive pair regex compiles")
    })
}

fn sensitive_json_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r#"(?i)("(?:api[_-]?key|access[_-]?key|secret[_-]?key|signature|sign|token|authorization|password|passwd)"\s*:\s*)("[^"]*"|[^,\s}]+)"#,
        )
        .expect("sensitive json regex compiles")
    })
}

fn bearer_token_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r#"(?i)\bBearer\s+[A-Za-z0-9._~+/\-=]+"#).expect("bearer token regex compiles")
    })
}

fn initial_rotation(log_path: &PathBuf) -> io::Result<()> {
    let previous = previous_log_path(log_path);

    if let Err(e) = fs::remove_file(&previous)
        && e.kind() != io::ErrorKind::NotFound
    {
        return Err(e);
    }
    if let Err(e) = fs::rename(log_path, &previous)
        && e.kind() != io::ErrorKind::NotFound
    {
        return Err(e);
    }
    Ok(())
}

fn previous_log_path(log_path: &Path) -> PathBuf {
    log_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("flowsurface-previous.log")
}

struct BackgroundLogger {
    sender: mpsc::Sender<LogMessage>,
    thread_handle: Option<JoinHandle<()>>,
}

impl BackgroundLogger {
    fn new(path: PathBuf) -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel();

        let thread_handle = thread::Builder::new()
            .name("logger-thread".to_string())
            .spawn(move || {
                let mut logger = match Logger::new(&path) {
                    Ok(logger) => logger,
                    Err(e) => {
                        eprintln!("Failed to initialize logger: {}", e);
                        return;
                    }
                };

                loop {
                    match receiver.recv() {
                        Ok(LogMessage::Content(data)) => {
                            if let Err(e) = logger.write_all(&data) {
                                eprintln!("Logging error: {}", e);
                            }
                        }
                        Ok(LogMessage::Flush) => {
                            if let Err(e) = logger.flush() {
                                eprintln!("Error flushing logs: {}", e);
                            }
                        }
                        Ok(LogMessage::Shutdown) | Err(_) => break,
                    }
                }
            })?;

        Ok(BackgroundLogger {
            sender,
            thread_handle: Some(thread_handle),
        })
    }
}

impl Write for BackgroundLogger {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = buf.len();
        self.sender
            .send(LogMessage::Content(buf.to_vec()))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "Logger thread disconnected"))?;
        Ok(len)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.sender
            .send(LogMessage::Flush)
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "Logger thread disconnected"))?;
        Ok(())
    }
}

impl Drop for BackgroundLogger {
    fn drop(&mut self) {
        let _ = self.sender.send(LogMessage::Shutdown);
        if let Some(handle) = self.thread_handle.take()
            && let Err(err) = handle.join()
        {
            eprintln!("Background logger thread panicked: {err:?}");
        }
    }
}

struct Logger {
    file: fs::File,
    path: PathBuf,
    current_size: u64,
}

impl Logger {
    fn new(path: &PathBuf) -> io::Result<Self> {
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;

        let size = file.metadata()?.len();

        Ok(Logger {
            file,
            path: path.clone(),
            current_size: size,
        })
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        fs::copy(&self.path, previous_log_path(&self.path))?;
        self.file.set_len(0)?;
        self.current_size = 0;
        Ok(())
    }
}

impl Write for Logger {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let buf_len = buf.len() as u64;

        if self.current_size + buf_len > MAX_LOG_FILE_SIZE {
            self.rotate()?;
        }

        let bytes = self.file.write(buf)?;
        self.current_size += bytes as u64;

        Ok(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

pub fn install_panic_hook() {
    static PANIC_HOOK: Once = Once::new();

    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();

        std::panic::set_hook(Box::new(move |panic_info| {
            let report = format_panic_report(panic_info);

            log::error!(target: "panic", "{report}");
            log::logger().flush();

            if let Err(err) = append_stderr_log_line(&report) {
                eprintln!("Failed to persist panic report: {err}");
            }

            previous(panic_info);
        }));
    });
}

pub fn report_stderr(message: &str) {
    if let Err(err) = append_stderr_log_line(message) {
        eprintln!("Failed to persist std log entry: {err}");
    }

    eprintln!("{message}");
}

fn format_panic_report(info: &PanicHookInfo<'_>) -> String {
    let current_thread = thread::current();
    let thread_name = current_thread.name().unwrap_or("unnamed");
    let location = info
        .location()
        .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
        .unwrap_or_else(|| "unknown location".to_string());

    let payload = info
        .payload()
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".to_string());

    let backtrace = Backtrace::force_capture();

    format!("panic in thread '{thread_name}' at {location}: {payload}\nBacktrace:\n{backtrace}")
}

fn append_stderr_log_line(message: &str) -> io::Result<()> {
    let log_path = data::log::path().map_err(|err| io::Error::other(err.to_string()))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;

    let message = redact_log_line(message);
    writeln!(
        file,
        "{}:FATAL -- {message}",
        chrono::Local::now().format("%H:%M:%S%.3f"),
    )?;

    file.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_export_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "flowsurface-log-export-test-{}",
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn redact_log_line_masks_credentials_and_signatures() {
        let line = "12:00:00.000:INFO [exchange] -- request api_key=access-123 secret_key=\"very-secret\" signature=deadbeef token: bearer-456 Authorization: Bearer session-secret password=hunter2";

        let redacted = redact_log_line(line);

        assert!(!redacted.contains("access-123"));
        assert!(!redacted.contains("very-secret"));
        assert!(!redacted.contains("deadbeef"));
        assert!(!redacted.contains("bearer-456"));
        assert!(!redacted.contains("session-secret"));
        assert!(!redacted.contains("hunter2"));
        assert!(redacted.contains("api_key=[redacted]"));
        assert!(redacted.contains("secret_key=[redacted]"));
        assert!(redacted.contains("signature=[redacted]"));
        assert!(redacted.contains("token=[redacted]"));
    }

    #[test]
    fn logger_rotates_at_size_limit_without_terminating() {
        let root = temp_export_root();
        fs::create_dir_all(&root).unwrap();
        let current = root.join("flowsurface-current.log");
        fs::write(&current, "before rotation\n").unwrap();

        let mut logger = Logger::new(&current).unwrap();
        logger.current_size = MAX_LOG_FILE_SIZE;
        logger.write_all(b"after rotation\n").unwrap();
        logger.flush().unwrap();

        assert_eq!(
            fs::read_to_string(previous_log_path(&current)).unwrap(),
            "before rotation\n"
        );
        assert_eq!(fs::read_to_string(&current).unwrap(), "after rotation\n");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn redact_log_line_masks_panic_shaped_credentials() {
        let redacted =
            redact_log_line("panic Authorization: Bearer panic-secret password=panic-password");

        assert!(!redacted.contains("panic-secret"));
        assert!(!redacted.contains("panic-password"));
    }

    #[test]
    fn export_logs_from_paths_writes_all_logs_redacted() {
        let root = temp_export_root();
        let input_dir = root.join("input");
        let output_dir = root.join("exports");
        fs::create_dir_all(&input_dir).unwrap();
        let current = input_dir.join("flowsurface-current.log");
        let previous = input_dir.join("flowsurface-previous.log");
        fs::write(
            &current,
            "10:00:00.001:INFO [app] -- started with token=current-secret\n",
        )
        .unwrap();
        fs::write(
            &previous,
            "09:59:00.001:ERROR [exchange] -- signature=previous-secret rejected\n",
        )
        .unwrap();

        let export =
            export_logs_from_paths(&[current, previous], &output_dir, LogExportMode::All).unwrap();
        let all_log = export.path.join("all.log");
        let content = fs::read_to_string(&all_log).unwrap();

        assert_eq!(export.files, vec![all_log]);
        assert!(content.contains("started with token=[redacted]"));
        assert!(content.contains("signature=[redacted] rejected"));
        assert!(!content.contains("current-secret"));
        assert!(!content.contains("previous-secret"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn export_logs_from_paths_groups_standard_sections() {
        let root = temp_export_root();
        let input_dir = root.join("input");
        let output_dir = root.join("exports");
        fs::create_dir_all(&input_dir).unwrap();
        let current = input_dir.join("flowsurface-current.log");
        fs::write(
            &current,
            concat!(
                "10:00:00.001:INFO [ui] -- pane layout changed\n",
                "10:00:00.002:INFO [connection] -- private websocket connected\n",
                "10:00:00.003:ERROR [exchange] -- /order/create rejected secret_key=hidden\n",
                "10:00:00.004:INFO [trading] -- MEXC_ORDER_REST_START symbol=BTC_USDT\n",
                "10:00:00.005:WARN [app] -- config saved\n",
            ),
        )
        .unwrap();

        let export =
            export_logs_from_paths(&[current], &output_dir, LogExportMode::Sections).unwrap();

        assert_eq!(export.files.len(), 5);
        assert!(
            fs::read_to_string(export.path.join("ui.log"))
                .unwrap()
                .contains("pane layout changed")
        );
        assert!(
            fs::read_to_string(export.path.join("connection.log"))
                .unwrap()
                .contains("private websocket connected")
        );
        let exchange_log = fs::read_to_string(export.path.join("exchange.log")).unwrap();
        assert!(exchange_log.contains("/order/create rejected secret_key=[redacted]"));
        assert!(!exchange_log.contains("hidden"));
        assert!(
            fs::read_to_string(export.path.join("trading.log"))
                .unwrap()
                .contains("MEXC_ORDER_REST_START")
        );
        assert!(
            fs::read_to_string(export.path.join("app.log"))
                .unwrap()
                .contains("config saved")
        );

        let _ = fs::remove_dir_all(root);
    }
}
