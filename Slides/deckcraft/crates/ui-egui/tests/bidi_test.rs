
#[test]
fn test_epaint_shaping() {
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(Default::default(), |ui| {
        let normal = "سلام";
        let reversed = "مالس";
        let font_id = egui::FontId::proportional(14.0);
        let g1 = ui.fonts_mut(|f| f.layout_no_wrap(normal.to_string(), font_id.clone(), egui::Color32::WHITE));
        let g2 = ui.fonts_mut(|f| f.layout_no_wrap(reversed.to_string(), font_id, egui::Color32::WHITE));
        println!("Normal glyphs:");
        for g in &g1.rows[0].glyphs { println!("  {} at {:?}", g.chr, g.pos); }
        println!("Reversed glyphs:");
        for g in &g2.rows[0].glyphs { println!("  {} at {:?}", g.chr, g.pos); }
    });
}
