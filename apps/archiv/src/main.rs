//! Archiv entry-point placeholder. No GUI, NAS access or file operations are implemented.

use std::io::{self, Write};

fn main() -> io::Result<()> {
    writeln!(io::stdout().lock(), "Archiv: preparation scaffold only. See README.md and ROADMAP.md.")
}
