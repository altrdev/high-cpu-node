//! Prints the reference of the endpoints in Markdown, from the catalog that `/help` is built from, so
//! the page of the documentation site is never out of date: `cargo run --example endpoints_reference`.
use torchio::config::Config;
use torchio::endpoints::catalog;

fn main() {
    // the defaults of the limits, with the control panel on so that /ui is in the list
    let config = Config::read(|name| (name == "UI").then(|| "1".to_string()), |_| {});
    println!("# Reference\n");
    println!(
        "Every endpoint with its parameters, as `/help` lists them, with the defaults and limits of a server with no settings. The ones marked *token* need it when [`TOKEN`](token.md) is set. The page [Endpoints](endpoints.md) says what each one does.\n"
    );

    println!("| Endpoint | Token | What it does |");
    println!("|----------|-------|--------------|");
    for endpoint in catalog(&config) {
        let token = if endpoint.auth { "yes" } else { "" };
        println!("| [`{0}`](#{1}) | {2} | {3} |", endpoint.path, anchor(endpoint.path), token, endpoint.description);
    }
    println!();

    for endpoint in catalog(&config) {
        if endpoint.path == "/" {
            println!("## The root, `/` {{#root}}\n");
        } else {
            println!("## `{}`\n", endpoint.path);
        }
        println!("{}.\n", endpoint.description.trim_end_matches('.'));
        if endpoint.params.is_empty() {
            println!("No parameters.\n");
            continue;
        }
        println!("| Parameter | In | Default | Range |");
        println!("|-----------|----|---------|-------|");
        for (name, param) in &endpoint.params {
            let place = endpoint.location.unwrap_or("query");
            let default = param.fallback.map_or("required".to_string(), |value| format!("`{value}`"));
            // the number of workers depends on the machine, and it is what WORKERS sets
            let max = if *name == "workers" { "WORKERS".to_string() } else { param.max.to_string() };
            println!("| `{name}` | {place} | {default} | `{}..{max}` |", param.min);
        }
        println!();
    }
}

// What mdBook makes of the heading `/status/:code`: the letters and digits, in lower case
fn anchor(path: &str) -> String {
    if path == "/" {
        return "root".to_string();
    }
    path.chars().filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_').collect::<String>().to_lowercase()
}
