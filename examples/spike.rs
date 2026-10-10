//! Spike page generator: `cargo run --example spike -- fixtures/sample.md > spike/out.html`.
//! Embeds the rendered HTML into `spike/index.html` (template) for manual selection tests.

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: spike <file.md>"))?;
    let raw = std::fs::read_to_string(&path)?;
    let rendered = annotator::render::render(&raw);
    let raw_json = serde_json::to_string(&raw)?.replace("</", "<\\/");
    let page = include_str!("../spike/index.html")
        .replace("<!--RENDERED-->", &rendered.html)
        .replace("\"__RAW__\"", &raw_json);
    print!("{page}");
    Ok(())
}
