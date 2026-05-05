use std::fmt::Write;
use std::path::Path;

use crate::adapters;

pub(crate) fn render_adapter_inventory(start: &Path) -> String {
    let adapters = adapters::list_adapters_for(start);

    let mut out = String::with_capacity(1024);
    let _ = writeln!(out, "shai adapters\n");
    let _ = writeln!(out, "  visible      {} total", adapters.len());

    if adapters.is_empty() {
        let _ = writeln!(out, "\n  no adapters visible");
        return out;
    }

    let _ = writeln!(out, "\n  adapters:");
    for adapter in adapters {
        let _ = writeln!(out, "    - {:<12} built-in", adapter.name);
    }

    out
}
