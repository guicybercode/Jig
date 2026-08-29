//! Entry point for the `cli-master-fake-agent` test fixture.

use std::io::{self, Write};
use std::process::ExitCode;

use cli_master_fake_agent::{Options, run_with_io};

fn main() -> ExitCode {
    let mut args = std::env::args();
    let _argv0 = args.next();
    let options = match Options::parse(args) {
        Ok(options) => options,
        Err(error) => {
            let _ = writeln!(io::stderr(), "cli-master-fake-agent: {error}");
            return ExitCode::from(2);
        }
    };

    match run_with_io(&options, &mut io::stdin(), &mut io::stdout()) {
        Ok(code) => {
            if let Ok(code) = u8::try_from(code) {
                ExitCode::from(code)
            } else {
                ExitCode::from(1)
            }
        }
        Err(error) => {
            let _ = writeln!(io::stderr(), "cli-master-fake-agent: {error}");
            ExitCode::from(1)
        }
    }
}
