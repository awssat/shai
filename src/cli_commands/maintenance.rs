use std::io::BufWriter;
use super::shared::local_shai_or_die;

pub(crate) fn cmd_gc(days: u32, delete: bool, dry_run: bool) {
    let (_shai_dir, db) = local_shai_or_die();
    let result = db.gc(days, delete, dry_run);

    if dry_run {
        println!("shai gc --dry-run (no changes made)\n");
    }

    if result.blob_count == 0 {
        println!("  ✅  nothing to gc — no blobs older than {} days", days);
        return;
    }

    let action = if dry_run {
        if delete { "would delete" } else { "would archive" }
    } else if delete { "deleted" } else { "archived" };

    println!("  {}  {} blob(s) ({:.1} MB) {}", if dry_run { "ℹ️ " } else { "✅ " }, result.blob_count, result.bytes_freed as f64 / 1_048_576.0, action);
}

pub(crate) fn cmd_export(output_path: &str) {
    let (_, db) = local_shai_or_die();
    let file = std::fs::File::create(output_path).unwrap_or_else(|e| {
        eprintln!("❌ Cannot create '{}': {}", output_path, e);
        std::process::exit(1);
    });
    let writer = BufWriter::new(file);

    match db.export_to(writer) {
        Ok(stats) => {
            println!("shai export\n");
            println!("  sessions exported   {}", stats.sessions);
            println!("  changes exported    {}", stats.changes);
        }
        Err(err) => eprintln!("❌ Export failed: {}", err),
    }
}

pub(crate) fn cmd_import(input_path: &str) {
    let (_, db) = local_shai_or_die();
    let file = std::fs::File::open(input_path).unwrap_or_else(|e| {
        eprintln!("❌ Cannot open '{}': {}", input_path, e);
        std::process::exit(1);
    });
    match db.import_from(file) {
        Ok(stats) => {
            println!("shai import\n");
            println!("  sessions imported   {}", stats.sessions_inserted);
        }
        Err(err) => eprintln!("❌ Import failed: {}", err),
    }
}
