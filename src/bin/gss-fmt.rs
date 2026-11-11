use std::env;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process;

use gss::{FormatOptions, unified_diff};

fn write_stdout(text: &str) {
    let stdout = io::stdout();
    let mut handle = io::BufWriter::new(stdout.lock());
    let _ = handle.write_all(text.as_bytes());
}

fn write_stderr(text: &str) {
    let stderr = io::stderr();
    let mut handle = io::BufWriter::new(stderr.lock());
    let _ = handle.write_all(text.as_bytes());
}

fn print_usage() {
    println!(
        "Usage: gss-fmt [OPTIONS] <file.gss|directory>...\n\n\
        Options:\n  \
        -w, --write, -i, --in-place   Write formatted result to source file(s) in place\n  \
        -c, --check                   Check if file(s) are formatted without modifying them\n  \
        -d, --diff                    Display unified diff of formatting changes\n  \
        --indent-width <N>            Indent width in spaces (default: 4)\n  \
        --tab-width <N>               Alias for --indent-width\n  \
        --use-tabs                    Use tabs for indentation instead of spaces\n  \
        --print-width <N>             Maximum target line width before wrapping lists (default: 80)\n  \
        --max-width <N>               Alias for --print-width\n  \
        --separate-blocks             Insert blank lines between top-level multiline blocks\n  \
        --allow-redefinition          Allow duplicate key redefinitions during parsing\n  \
        --color <always|auto|never>   Control colorized diff output (default: auto)\n  \
        -v, --version                 Display version information\n  \
        -h, --help                    Display this help message"
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorChoice {
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    fn should_color(self) -> bool {
        match self {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => io::stdout().is_terminal(),
        }
    }
}

fn collect_gss_files(path: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    if path.is_file() {
        out.push(path.to_path_buf());
        return Ok(());
    }
    if path.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(path)?.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            let child = entry.path();
            if child.is_dir() {
                collect_gss_files(&child, out)?;
            } else if child.extension().and_then(|ext| ext.to_str()) == Some("gss") {
                out.push(child);
            }
        }
    }
    Ok(())
}

pub fn run() {
    let args: Vec<String> = env::args().skip(1).collect();

    let mut write_in_place = false;
    let mut check_only = false;
    let mut show_diff = false;
    let mut color_choice = ColorChoice::Auto;
    let mut format_options = FormatOptions::default();
    let mut raw_inputs = Vec::new();

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            "-v" | "--version" => {
                println!("gss-fmt {}", env!("CARGO_PKG_VERSION"));
                process::exit(0);
            }
            "-w" | "--write" | "-i" | "--in-place" => {
                write_in_place = true;
            }
            "-c" | "--check" => {
                check_only = true;
            }
            "-d" | "--diff" => {
                show_diff = true;
            }
            "--use-tabs" => {
                format_options.use_tabs = true;
            }
            "--separate-blocks" => {
                format_options.separate_blocks = true;
            }
            "--allow-redefinition" => {
                format_options.allow_redefinition = true;
            }
            "--indent-width" | "--tab-width" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: {arg} requires a positive integer argument");
                    process::exit(1);
                }
                match args[i].parse::<usize>() {
                    Ok(val) if val > 0 => format_options.indent_width = val,
                    _ => {
                        eprintln!("Error: {arg} requires a positive integer argument");
                        process::exit(1);
                    }
                }
            }
            "--print-width" | "--max-width" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: {arg} requires a positive integer argument");
                    process::exit(1);
                }
                match args[i].parse::<usize>() {
                    Ok(val) if val > 0 => format_options.print_width = val,
                    _ => {
                        eprintln!("Error: {arg} requires a positive integer argument");
                        process::exit(1);
                    }
                }
            }
            "--color" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --color requires one of: auto, always, never");
                    process::exit(1);
                }
                match args[i].as_str() {
                    "always" => color_choice = ColorChoice::Always,
                    "never" => color_choice = ColorChoice::Never,
                    "auto" => color_choice = ColorChoice::Auto,
                    other => {
                        eprintln!(
                            "Error: invalid --color value `{other}`, expected: auto, always, never"
                        );
                        process::exit(1);
                    }
                }
            }
            "-" => {
                raw_inputs.push("-".to_string());
            }
            other if other.starts_with('-') => {
                eprintln!("Unknown option: {other}");
                eprintln!("For help, run: gss-fmt --help");
                process::exit(1);
            }
            file_or_dir => {
                raw_inputs.push(file_or_dir.to_string());
            }
        }
        i += 1;
    }

    if raw_inputs.is_empty() {
        if io::stdin().is_terminal() {
            eprintln!("Error: No input files specified.");
            eprintln!("Usage: gss-fmt [OPTIONS] <file.gss|directory>...");
            process::exit(1);
        } else {
            raw_inputs.push("-".to_string());
        }
    }

    let color = color_choice.should_color();
    let mut has_error = false;
    let mut unformatted_files = Vec::new();

    for input in &raw_inputs {
        if input == "-" {
            let mut buffer = String::new();
            if let Err(e) = io::stdin().read_to_string(&mut buffer) {
                eprintln!("Error reading stdin: {e}");
                has_error = true;
                continue;
            }
            match gss::format_str_with_config(&buffer, &format_options) {
                Ok(formatted) => {
                    let is_unformatted = formatted != buffer;
                    if is_unformatted {
                        unformatted_files.push("<stdin>".to_string());
                        if show_diff {
                            let diff =
                                unified_diff(&buffer, &formatted, "a/<stdin>", "b/<stdin>", color);
                            write_stdout(&diff);
                        }
                    }
                    if !check_only && !show_diff {
                        write_stdout(&formatted);
                    }
                }
                Err(err) => {
                    eprintln!("stdin: {err}");
                    has_error = true;
                }
            }
            continue;
        }

        let path = Path::new(input);
        if !path.exists() {
            eprintln!("{input}: No such file or directory");
            has_error = true;
            continue;
        }

        let mut files_to_format = Vec::new();
        if let Err(err) = collect_gss_files(path, &mut files_to_format) {
            eprintln!("{input}: {err}");
            has_error = true;
            continue;
        }

        if files_to_format.is_empty() && path.is_file() {
            files_to_format.push(path.to_path_buf());
        }

        for file_path in files_to_format {
            let display_name = file_path.display().to_string();
            let content = match fs::read_to_string(&file_path) {
                Ok(c) => c,
                Err(err) => {
                    eprintln!("{display_name}: {err}");
                    has_error = true;
                    continue;
                }
            };

            match gss::format_str_with_config(&content, &format_options) {
                Ok(formatted) => {
                    let is_unformatted = formatted != content;
                    if is_unformatted {
                        unformatted_files.push(display_name.clone());
                        if show_diff {
                            let diff = unified_diff(
                                &content,
                                &formatted,
                                &format!("a/{display_name}"),
                                &format!("b/{display_name}"),
                                color,
                            );
                            write_stdout(&diff);
                        }
                    }

                    if write_in_place {
                        if is_unformatted && let Err(err) = fs::write(&file_path, &formatted) {
                            eprintln!("Error writing {display_name}: {err}");
                            has_error = true;
                        }
                    } else if !check_only && !show_diff {
                        write_stdout(&formatted);
                    }
                }
                Err(err) => {
                    eprintln!("{display_name}: {err}");
                    has_error = true;
                }
            }
        }
    }

    if check_only {
        if !unformatted_files.is_empty() {
            for unformatted in &unformatted_files {
                let msg = format!("File not formatted: {unformatted}\n");
                write_stderr(&msg);
            }
            process::exit(1);
        }
    } else if show_diff && !write_in_place && !unformatted_files.is_empty() {
        process::exit(1);
    }

    if has_error {
        process::exit(1);
    }
}

#[allow(dead_code)]
fn main() {
    run();
}
