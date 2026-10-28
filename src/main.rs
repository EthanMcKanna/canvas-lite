mod api;
mod app;
mod exams;
mod html;
mod names;
mod store;
mod theme;
mod tray;
mod web;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    match args.as_slice() {
        [_, flag, base] if flag == "--login" => {
            web::login(base);
            return Ok(());
        }
        [_, flag, url] if flag == "--extract" => {
            web::extract(url);
            return Ok(());
        }
        _ => {}
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Canvas Lite")
            .with_inner_size([1240.0, 820.0])
            .with_min_inner_size([860.0, 540.0])
            // Edge-to-edge content with the traffic lights floating over the sidebar.
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false),
        // OpenGL rather than wgpu/Metal: on ProMotion displays wgpu's Metal path is pinned to
        // 60 Hz and re-allocates drawables on every live-resize step, which made resizing
        // visibly lag. glow runs at the display's refresh rate and resizes smoothly.
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native("Canvas Lite", options, Box::new(|cc| Ok(Box::new(app::App::new(cc)))))
}
