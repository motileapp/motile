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

fn scene(quads: &[Quad]) -> Scene {
    let mut scene = Scene::default();
    for quad in quads {
        scene.insert_primitive(*quad);
    }
    scene.finish();
    scene
}

#[test]
fn identical_scenes_are_unchanged() {
    let quads = [quad(rect(0., 0., 100., 100.), 0.5)];
    assert_eq!(SceneDamage::between(&scene(&quads), &scene(&quads)), SceneDamage::Unchanged);
}

#[test]
fn a_changed_quad_damages_its_bounds() {
    let still = quad(rect(0., 0., 100., 100.), 0.1);
    let before = scene(&[still, quad(rect(200., 200., 10., 20.), 0.5)]);
    let after = scene(&[still, quad(rect(200., 200., 10., 20.), 0.9)]);
    assert_eq!(SceneDamage::between(&before, &after), SceneDamage::Rect(rect(200., 200., 10., 20.)));
}

#[test]
fn a_caret_damages_only_itself_appearing_and_disappearing() {
    let left = quad(rect(0., 0., 50., 50.), 0.1);
    let caret = quad(rect(60., 0., 2., 20.), 0.5);
    let right = quad(rect(100., 0., 50., 50.), 0.9);
    let off = scene(&[left, right]);
    let on = scene(&[left, caret, right]);
    assert_eq!(SceneDamage::between(&off, &on), SceneDamage::Rect(rect(60., 0., 2., 20.)));
    assert_eq!(SceneDamage::between(&on, &off), SceneDamage::Rect(rect(60., 0., 2., 20.)));
}

#[test]
fn a_rotated_sprite_damages_where_it_is_drawn() {
    let quarter_turn = TransformationMatrix { rotation_scale: [[0., -1.], [1., 0.]], translation: [100., 0.] };
    let sprite = |color: Hsla| MonochromeSprite {
        order: 0,
        pad: 0,
        bounds: rect(10., 0., 20., 10.),
        content_mask: ContentMask { bounds: rect(0., 0., 1000., 1000.) },
        color,
        tile: AtlasTile {
            texture_id: AtlasTextureId { index: 0, kind: AtlasTextureKind::Monochrome },
            tile_id: TileId(0),
            padding: 0,
            bounds: Default::default(),
        },
        transformation: quarter_turn,
    };
    let mut before = Scene::default();
    before.insert_primitive(sprite(Hsla::black()));
    before.finish();
    let mut after = Scene::default();
    after.insert_primitive(sprite(Hsla::white()));
    after.finish();

    let corners = [(10., 0.), (30., 0.), (10., 10.), (30., 10.)]
        .map(|(x, y)| quarter_turn.apply(Point::new(gpui_kit::px(x), gpui_kit::px(y))));
    let left = corners.iter().map(|p| f32::from(p.x)).fold(f32::MAX, f32::min);
    let top = corners.iter().map(|p| f32::from(p.y)).fold(f32::MAX, f32::min);
    let right = corners.iter().map(|p| f32::from(p.x)).fold(f32::MIN, f32::max);
    let bottom = corners.iter().map(|p| f32::from(p.y)).fold(f32::MIN, f32::max);
    assert_eq!(SceneDamage::between(&before, &after), SceneDamage::Rect(rect(left, top, right - left, bottom - top)));
}

#[test]
fn union_covers_both() {
    assert_eq!(SceneDamage::Full.union(SceneDamage::Rect(rect(0., 0., 1., 1.))), SceneDamage::Full);
    assert_eq!(SceneDamage::Unchanged.union(SceneDamage::Unchanged), SceneDamage::Unchanged);
    assert_eq!(
        SceneDamage::Rect(rect(0., 0., 10., 10.)).union(SceneDamage::Rect(rect(20., 20., 10., 10.))),
        SceneDamage::Rect(rect(0., 0., 30., 30.))
    );
}
