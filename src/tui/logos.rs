use image::{DynamicImage, RgbaImage};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};

const CODEX_SVG: &str = include_str!("../../assets/codex.svg");
const OPENCODE_SVG: &str = include_str!("../../assets/opencode.svg");
const ANTIGRAVITY_SVG: &str = include_str!("../../assets/antigravity.svg");

pub fn provider_logo(provider_id: &str, picker: &Picker) -> Option<StatefulProtocol> {
    let svg = match provider_id {
        "antigravity" => ANTIGRAVITY_SVG,
        "codex" => CODEX_SVG,
        "opencode" => OPENCODE_SVG,
        _ => return None,
    };
    let image = render_svg(svg)?;
    Some(picker.new_resize_protocol(image))
}

fn render_svg(svg: &str) -> Option<DynamicImage> {
    let svg = svg.replace("currentColor", "#FFFFFF");
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    let image = RgbaImage::from_raw(size.width(), size.height(), pixmap.take())?;
    Some(DynamicImage::ImageRgba8(image))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_provider_logos_rasterize() {
        let normalized = CODEX_SVG.replace("currentColor", "#FFFFFF");
        let parsed = resvg::usvg::Tree::from_str(&normalized, &resvg::usvg::Options::default());
        assert!(parsed.is_ok(), "Codex SVG parse failed: {:?}", parsed.err());
        assert!(render_svg(CODEX_SVG).is_some());
        assert!(render_svg(OPENCODE_SVG).is_some());
        assert!(render_svg(ANTIGRAVITY_SVG).is_some());
    }
}
