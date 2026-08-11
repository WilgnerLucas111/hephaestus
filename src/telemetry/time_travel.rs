use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

/// Represents a local variable in a stack frame.
#[derive(Clone, Debug)]
pub struct LocalVariable {
    /// Variable name
    pub name: String,
    /// Source-level type
    pub type_name: String,
    /// Memory address (if known)
    pub address: Option<u64>,
    /// Value (string representation)
    pub value: String,
}

/// Represents a region of heap memory.
#[derive(Clone, Debug)]
pub struct HeapRegion {
    /// Start address
    pub start_addr: u64,
    /// Size in bytes
    pub size: usize,
    /// Captured data (truncated if large)
    pub data: Vec<u8>,
}

/// Represents CPU registers (x86-64).
#[derive(Clone, Debug)]
pub struct RegisterSnapshot {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
}

/// Represents process information.
#[derive(Clone, Debug)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub uid: u32,
    pub gid: u32,
}

/// Represents a stack frame snapshot.
#[derive(Clone, Debug)]
pub struct StackFrameSnapshot {
    /// Function name (from symbol table)
    pub function_name: String,
    /// Source file path
    pub file_path: String,
    /// Line number in source
    pub line_number: u32,
    /// Column number
    pub column: u32,
    /// Local variables captured (name → serialized value)
    pub local_variables: HashMap<String, LocalVariable>,
    /// Return address (PC)
    pub instruction_pointer: u64,
    /// Frame pointer
    pub frame_pointer: u64,
}

/// Complete snapshot of execution state at panic moment
#[derive(Clone, Debug)]
pub struct TimeTravelSnapshot {
    /// Stack frames with local variables (if debuginfo available)
    pub stack_frames: Vec<StackFrameSnapshot>,
    /// Key memory regions from heap
    pub heap_regions: Vec<HeapRegion>,
    /// CPU registers (x86_64, ARM64, etc.)
    pub registers: RegisterSnapshot,
    /// Current working directory
    pub cwd: PathBuf,
    /// Environment variables
    pub env_vars: HashMap<String, String>,
    /// Process ID and parent PID
    pub process_info: ProcessInfo,
    /// System time at capture
    pub system_time_ns: u64,
}

/// Telemetry system for capturing execution state at panic moments.
#[derive(Clone)]
pub struct TimeTravelTelemetry;

impl TimeTravelTelemetry {
    /// Capture complete execution state at panic moment
    pub fn capture_at_panic() -> Result<TimeTravelSnapshot, String> {
        // Step 1: Capture backtrace with frame info
        let backtrace = backtrace::Backtrace::new();
        let stack_frames = Self::extract_stack_frames(&backtrace)?;

        // Step 2: Capture heap regions (from /proc/self/maps)
        let heap_regions = Self::read_heap_regions(process::id())?;

        // Step 3: Capture registers (x86-64 only for now)
        let registers = Self::capture_registers();

        // Step 4: Capture process environment (filtered with secret redaction)
        let cwd =
            env::current_dir().map_err(|e| format!("Failed to get current directory: {}", e))?;
        let env_vars = Self::capture_safe_env_vars();

        // Step 5: Process info
        let process_info = Self::get_process_info()?;

        // Step 6: System time
        let system_time_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("Failed to get system time: {}", e))?
            .as_nanos() as u64;

        Ok(TimeTravelSnapshot {
            stack_frames,
            heap_regions,
            registers,
            cwd,
            env_vars,
            process_info,
            system_time_ns,
        })
    }

    /// Captures safe environment variables and redacts secret values.
    pub fn capture_safe_env_vars() -> HashMap<String, String> {
        let allowlist = [
            "RUST_LOG",
            "RUST_BACKTRACE",
            "APP_ENV",
            "FEATURE_FLAGS",
            "CARGO_PKG_NAME",
            "CARGO_PKG_VERSION",
            "PATH",
            "LANG",
        ];

        let secret_keywords = [
            "_TOKEN",
            "_SECRET",
            "_PASSWORD",
            "_KEY",
            "DATABASE_URL",
            "CREDENTIAL",
            "AUTH",
        ];

        let mut safe_vars = HashMap::new();
        for (key, val) in env::vars() {
            let key_upper = key.to_uppercase();

            // Check if key is explicitly in allowlist
            let is_allowed = allowlist.iter().any(|&allowed| key_upper == allowed);

            if is_allowed {
                // Check if value contains secrets
                let is_secret = secret_keywords.iter().any(|&k| key_upper.contains(k));
                if is_secret {
                    safe_vars.insert(key, "[REDACTED]".to_string());
                } else {
                    safe_vars.insert(key, val);
                }
            }
        }

        safe_vars
    }

    /// Extract stack frames with local variables (requires debuginfo)
    fn extract_stack_frames(
        backtrace: &backtrace::Backtrace,
    ) -> Result<Vec<StackFrameSnapshot>, String> {
        let mut frames = Vec::new();

        // We'll use the backtrace crate's symbol resolution if available
        // Note: This requires the `backtrace` crate to be built with the `symbol` feature.
        for frame in backtrace.frames() {
            // Frame symbols (function name, file, line)
            for symbol in frame.symbols() {
                let function_name = symbol
                    .name()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                let file_path = symbol
                    .filename()
                    .and_then(|p| p.to_str().map(String::from))
                    .unwrap_or_else(|| "unknown".to_string());

                let line_number = symbol.lineno().unwrap_or(0);
                let column = 0; // We don't have column information from backtrace

                // TODO: Parse local variables from debug symbols (DWARF)
                let local_variables = HashMap::new();

                let instruction_pointer = frame.ip() as u64;
                let frame_pointer = 0; // TODO: extract from registers or frame pointer

                frames.push(StackFrameSnapshot {
                    function_name,
                    file_path,
                    line_number,
                    column,
                    local_variables,
                    instruction_pointer,
                    frame_pointer,
                });
            }
        }

        Ok(frames)
    }

    /// Read heap memory regions from /proc/$pid/maps
    fn read_heap_regions(pid: u32) -> Result<Vec<HeapRegion>, String> {
        let maps_path = format!("/proc/{}/maps", pid);
        let maps_content =
            fs::read_to_string(&maps_path).map_err(|_| format!("Could not read {}", maps_path))?;

        let mut regions = Vec::new();

        for line in maps_content.lines() {
            // Parse line: "start-end perm offset dev inode pathname"
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 6 {
                continue;
            }

            let addr_range = parts[0];
            let perm = parts[1];

            // Parse address range: "start-end"
            let addr_parts: Vec<&str> = addr_range.split('-').collect();
            if addr_parts.len() != 2 {
                continue;
            }

            let start = u64::from_str_radix(addr_parts[0], 16)
                .map_err(|_| format!("Invalid start address in {}", addr_range))?;
            let end = u64::from_str_radix(addr_parts[1], 16)
                .map_err(|_| format!("Invalid end address in {}", addr_range))?;

            // Only capture writable regions (heap)
            if perm.contains('w') {
                let size = (end - start) as usize;

                // Limit capture to avoid huge dumps (e.g., 4KB per region)
                let data_size = usize::min(size, 4096);

                // We cannot safely read arbitrary memory addresses in a portable way.
                // In a real implementation, we would use `process_vm_readv` or similar,
                // but for simplicity and safety, we'll zero out the data.
                // TODO: Implement safe memory reading for the heap regions.
                let data = vec![0u8; data_size];

                regions.push(HeapRegion {
                    start_addr: start,
                    size,
                    data,
                });
            }
        }

        Ok(regions)
    }

    /// Capture CPU registers (x86-64)
    fn capture_registers() -> RegisterSnapshot {
        // Note: This is a placeholder. In a real implementation, we would use
        // platform-specific assembly or system calls to capture the register state
        // at the point of the panic. However, capturing registers from a signal handler
        // or after a panic is complex and may require platform-specific code.

        // For now, we return zeros. This is not ideal but allows the code to compile
        // and run on platforms where we don't have register capture implemented.
        RegisterSnapshot {
            rax: 0,
            rbx: 0,
            rcx: 0,
            rdx: 0,
            rsi: 0,
            rdi: 0,
            rbp: 0,
            rsp: 0,
            r8: 0,
            r9: 0,
            r10: 0,
            r11: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
        }
    }

    /// Get process information (PID, PPID, UID, GID)
    fn get_process_info() -> Result<ProcessInfo, String> {
        let pid = process::id();

        // Getting PPID is Linux-specific: read from /proc/self/stat
        let ppid = Self::get_ppid()?;

        // Get UID and GID using libc
        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };

        Ok(ProcessInfo {
            pid,
            ppid: ppid as u32,
            uid: uid as u32,
            gid: gid as u32,
        })
    }

    /// Get parent PID from /proc/self/stat
    fn get_ppid() -> Result<u64, String> {
        let stat_path = "/proc/self/stat";
        let stat_content =
            fs::read_to_string(stat_path).map_err(|_| format!("Could not read {}", stat_path))?;

        // The format of /proc/self/stat is:
        // pid (1) comm (2) state (3) ppid (4) ...
        let parts: Vec<&str> = stat_content.split_whitespace().collect();
        if parts.len() < 4 {
            return Err(format!("Invalid format in {}", stat_path));
        }

        let ppid_str = parts[3];
        ppid_str
            .parse::<u64>()
            .map_err(|_| format!("Invalid PPID in {}", stat_path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_at_panic() -> Result<(), String> {
        // This test will run in a normal context, not a panic.
        // We just want to see if the function returns without error.
        let snapshot = TimeTravelTelemetry::capture_at_panic()?;
        // We expect at least some data: cwd, env_vars, etc.
        assert!(!snapshot.cwd.to_string_lossy().is_empty());
        assert!(!snapshot.env_vars.is_empty());
        assert_eq!(snapshot.process_info.pid, std::process::id());
        Ok(())
    }

    #[test]
    fn test_get_ppid() -> Result<(), String> {
        let ppid = TimeTravelTelemetry::get_ppid()?;
        // PPID should be a positive number
        assert!(ppid > 0);
        Ok(())
    }
}
