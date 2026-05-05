use crate::discovery;

pub(crate) fn cmd_adapters_list() {
    let cwd = std::env::current_dir().unwrap_or_default();
    println!("{}", discovery::render_adapter_inventory(&cwd));
}
