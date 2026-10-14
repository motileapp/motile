use gpui_kit::{
    AtlasTextureId, AtlasTextureKind, AtlasTile, Bounds, ContentMask, Hsla, MonochromeSprite, Point, Quad,
    ScaledPixels, Scene, SceneDamage, Size, TileId, TransformationMatrix,
};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Bounds<ScaledPixels> {
    Bounds {
        origin: Point { x: ScaledPixels(x), y: ScaledPixels(y) },
        size: Size { width: ScaledPixels(width), height: ScaledPixels(height) },
    }
}

fn quad(bounds: Bounds<ScaledPixels>, lightness: f32) -> Quad {
    Quad {
        bounds,
        content_mask: ContentMask { bounds: rect(0., 0., 1000., 1000.) },
        background: Hsla { h: 0., s: 0., l: lightness, a: 1. }.into(),
        ..Default::default()
    }
}

fn glyph(bounds: Bounds<ScaledPixels>, tile: u32, transformation: TransformationMatrix) -> MonochromeSprite {
    MonochromeSprite {
        order: 0,
        pad: 0,
        bounds,
        content_mask: ContentMask { bounds: rect(0., 0., 1000., 1000.) },
        color: Hsla::black(),
        tile: AtlasTile {
            texture_id: AtlasTextureId { index: 0, kind: AtlasTextureKind::Monochrome },
            tile_id: TileId(tile),
            padding: 0,
            bounds: Default::default(),
        },
        transformation,
    }
}

fn scene(quads: &[Quad], glyphs: &[MonochromeSprite]) -> Scene {
    let mut scene = Scene::default();
    for quad in quads {
        scene.insert_primitive(*quad);
    }
    for glyph in glyphs {
        scene.insert_primitive(*glyph);
    }
    scene.finish();
    scene
}

fn rects(damage: SceneDamage) -> Vec<Bounds<ScaledPixels>> {
    let SceneDamage::Rects(mut rects) = damage else {
        panic!("expected rects, got {damage:?}");
    };
    rects.sort_by(|a, b| a.origin.x.0.total_cmp(&b.origin.x.0));
    rects
}

#[test]
fn identical_scenes_are_unchanged() {
    let quads = [quad(rect(0., 0., 100., 100.), 0.5)];
    assert_eq!(SceneDamage::between(&scene(&quads, &[]), &scene(&quads, &[])), SceneDamage::Unchanged);
}

#[test]
fn a_changed_quad_damages_its_bounds() {
    let still = quad(rect(0., 0., 100., 100.), 0.1);
    let before = scene(&[still, quad(rect(200., 200., 10., 20.), 0.5)], &[]);
    let after = scene(&[still, quad(rect(200., 200., 10., 20.), 0.9)], &[]);
    assert_eq!(rects(SceneDamage::between(&before, &after)), [rect(200., 200., 10., 20.)]);
}

#[test]
fn a_caret_damages_only_itself_appearing_and_disappearing() {
    let left = quad(rect(0., 0., 50., 50.), 0.1);
    let caret = quad(rect(60., 0., 2., 20.), 0.5);
    let right = quad(rect(100., 0., 50., 50.), 0.9);
    let off = scene(&[left, right], &[]);
    let on = scene(&[left, caret, right], &[]);
    assert_eq!(rects(SceneDamage::between(&off, &on)), [rect(60., 0., 2., 20.)]);
    assert_eq!(rects(SceneDamage::between(&on, &off)), [rect(60., 0., 2., 20.)]);
}

#[test]
fn a_replaced_glyph_damages_only_itself_wherever_its_tile_sorts() {
    let unit = TransformationMatrix::unit();
    let sidebar = [glyph(rect(10., 10., 8., 10.), 1, unit), glyph(rect(10., 300., 8., 10.), 3, unit)];
    let next_glyph = glyph(rect(508., 10., 8., 10.), 2, unit);
    let before = [sidebar[0], sidebar[1], glyph(rect(500., 10., 8., 10.), 0, unit), next_glyph];
    let after = [sidebar[0], sidebar[1], glyph(rect(500., 10., 8., 10.), 3, unit), next_glyph];
    assert_eq!(rects(SceneDamage::between(&scene(&[], &before), &scene(&[], &after))), [rect(500., 10., 8., 10.)]);
}

#[test]
fn changes_far_apart_damage_separate_rects() {
    let before = scene(&[quad(rect(10., 10., 50., 10.), 0.5), quad(rect(500., 600., 50., 10.), 0.5)], &[]);
    let after = scene(&[quad(rect(10., 10., 50., 10.), 0.9), quad(rect(500., 600., 50., 10.), 0.9)], &[]);
    assert_eq!(rects(SceneDamage::between(&before, &after)), [rect(10., 10., 50., 10.), rect(500., 600., 50., 10.)]);
}

#[test]
fn a_rotated_sprite_damages_where_it_is_drawn() {
    let quarter_turn = TransformationMatrix { rotation_scale: [[0., -1.], [1., 0.]], translation: [100., 0.] };
    let shown = glyph(rect(10., 0., 20., 10.), 0, quarter_turn);
    let recolored = MonochromeSprite { color: Hsla::white(), ..shown };

    let corners = [(10., 0.), (30., 0.), (10., 10.), (30., 10.)]
        .map(|(x, y)| quarter_turn.apply(Point::new(gpui_kit::px(x), gpui_kit::px(y))));
    let left = corners.iter().map(|p| f32::from(p.x)).fold(f32::MAX, f32::min);
    let top = corners.iter().map(|p| f32::from(p.y)).fold(f32::MAX, f32::min);
    let right = corners.iter().map(|p| f32::from(p.x)).fold(f32::MIN, f32::max);
    let bottom = corners.iter().map(|p| f32::from(p.y)).fold(f32::MIN, f32::max);
    assert_eq!(
        rects(SceneDamage::between(&scene(&[], &[shown]), &scene(&[], &[recolored]))),
        [rect(left, top, right - left, bottom - top)]
    );
}

#[test]
fn union_covers_both() {
    assert_eq!(SceneDamage::Full.union(SceneDamage::Rects(vec![rect(0., 0., 1., 1.)])), SceneDamage::Full);
    assert_eq!(SceneDamage::Unchanged.union(SceneDamage::Unchanged), SceneDamage::Unchanged);
    assert_eq!(
        SceneDamage::Rects(vec![rect(0., 0., 10., 10.)]).union(SceneDamage::Rects(vec![rect(5., 5., 10., 10.)])),
        SceneDamage::Rects(vec![rect(0., 0., 15., 15.)])
    );
    assert_eq!(
        SceneDamage::Rects(vec![rect(0., 0., 10., 10.), rect(500., 500., 10., 10.)])
            .union(SceneDamage::Rects(vec![rect(0., 0., 600., 600.)])),
        SceneDamage::Rects(vec![rect(0., 0., 600., 600.)])
    );
}
