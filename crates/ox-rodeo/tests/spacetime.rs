use std::f64::consts::PI;
use std::path::PathBuf;

use ox_library::color::Color;
use ox_library::geometry::Rect;
use ox_rodeo::draw::DrawList;
use ox_rodeo::draw::raster::render as raster;
use ox_rodeo::spacetime::{Disk, Scene, Sky, Star, deflection, render};
use ox_rodeo::texture::Textures;

fn luminance(color: Color) -> f32 {
    0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b
}

fn glowing() -> Sky {
    Sky {
        stars: 0.0,
        band: 0.0,
        nebulae: 0.0,
        glow: 1.0,
        ..Sky::default()
    }
}

fn smooth_disk() -> Disk {
    Disk {
        turbulence: 0.0,
        ..Disk::default()
    }
}

#[test]
fn far_rays_bend_by_two_over_their_impact_parameter() {
    for impact in [20.0, 50.0, 200.0] {
        let bent = deflection(impact).unwrap();
        let expected = 2.0 / impact + 15.0 * PI / 16.0 / (impact * impact);
        assert!(
            (bent - expected).abs() < expected * 0.02,
            "{impact}: {bent} against {expected}"
        );
    }
}

#[test]
fn rays_inside_the_photon_sphere_fall_in() {
    let critical = 1.5 * 3f64.sqrt();

    assert_eq!(deflection(critical - 0.01), None);
    assert!(deflection(critical + 0.01).unwrap() > PI);
}

#[test]
fn a_distant_camera_sees_a_shadow_as_wide_as_the_critical_impact_parameter() {
    let scene = Scene {
        distance: 1000.0,
        fov: 0.02,
        sky: Some(glowing()),
        ..Scene::default()
    };
    let pixels = render(&scene, 201, 201);

    let edge = (0..100)
        .find(|x| luminance(pixels[100 * 201 + 100 + x]) > 0.1)
        .unwrap();
    let focal = 100.5 / 0.01f64.tan();
    let angle = (f64::from(u32::try_from(edge).unwrap()) / focal).atan();
    let impact = 1000.0 * angle.sin() / (1.0 - 1.0 / 1000.0f64).sqrt();
    assert!((impact - 1.5 * 3f64.sqrt()).abs() < 0.12, "{impact}");
}

fn halves(scene: &Scene) -> (f32, f32) {
    let (width, height) = (120, 80);
    let pixels = render(scene, width, height);
    let mut sums = (0.0, 0.0);
    for (index, color) in pixels.iter().enumerate() {
        if index % 120 < 60 {
            sums.0 += luminance(*color);
        } else {
            sums.1 += luminance(*color);
        }
    }
    sums
}

#[test]
fn the_side_of_the_disk_coming_toward_the_camera_is_brighter() {
    let scene = |shifted| Scene {
        tilt: 0.25,
        distance: 60.0,
        fov: 0.6,
        sky: None,
        disk: Some(smooth_disk()),
        doppler: shifted,
        redshift: shifted,
        ..Scene::default()
    };

    let (left, right) = halves(&scene(true));
    assert!(left.max(right) > 1.5 * left.min(right), "{left} {right}");
    let (left, right) = halves(&scene(false));
    assert!(
        (left - right).abs() < 0.05 * left.max(right),
        "{left} {right}"
    );
}

#[test]
fn a_shrinking_star_reddens_and_dims_toward_its_horizon() {
    let seen = |radius| {
        let scene = Scene {
            star: Some(Star {
                radius,
                ..Star::default()
            }),
            sky: None,
            distance: 60.0,
            fov: 0.5,
            ..Scene::default()
        };
        render(&scene, 41, 41)[20 * 41 + 20]
    };
    let colors = [8.0, 3.0, 1.5, 1.1, 1.01].map(seen);

    for pair in colors.windows(2) {
        assert!(luminance(pair[0]) > luminance(pair[1]), "{colors:?}");
    }
    assert!(luminance(colors[4]) < 0.02, "{colors:?}");
    assert!(
        colors[3].r / colors[3].b.max(0.001) > colors[0].r / colors[0].b,
        "{colors:?}"
    );
}

#[test]
fn lensing_lifts_the_far_side_of_the_disk_over_the_shadow() {
    let above = |lensing| {
        let scene = Scene {
            lensing,
            tilt: 0.12,
            distance: 60.0,
            fov: 0.6,
            sky: None,
            disk: Some(smooth_disk()),
            ..Scene::default()
        };
        let pixels = render(&scene, 121, 121);
        (40..48)
            .filter(|row| luminance(pixels[row * 121 + 60]) > 0.05)
            .count()
    };

    assert!(above(true) > 0);
    assert_eq!(above(false), 0);
}

#[test]
fn an_unlit_disk_hides_nothing_and_a_dim_one_fades_in() {
    let scene = |brightness: Option<f64>| Scene {
        disk: brightness.map(|brightness| Disk {
            brightness,
            ..Disk::default()
        }),
        ..Scene::default()
    };
    let total = |pixels: Vec<Color>| pixels.into_iter().map(luminance).sum::<f32>();

    assert_eq!(
        render(&scene(Some(0.0)), 64, 36),
        render(&scene(None), 64, 36)
    );
    let dark = |brightness| Scene {
        sky: None,
        ..scene(Some(brightness))
    };
    let (none, half, full) = (
        total(render(&dark(0.0), 64, 36)),
        total(render(&dark(0.5), 64, 36)),
        total(render(&dark(1.0), 64, 36)),
    );
    assert!(none < half && half < full, "{none} {half} {full}");
}

#[test]
fn black_holes_draw_into_their_rect_under_later_layers() {
    let mut list = DrawList::new();
    list.background = Color::rgba8(10, 20, 30, 255);
    list.black_hole(
        Rect::new(2.0, 2.0, 20.0, 20.0),
        Scene {
            distance: 60.0,
            fov: 0.3,
            sky: Some(glowing()),
            ..Scene::default()
        },
        0,
    );
    let frame = raster(&mut list, &Textures::new(), 24, 24);
    let at = |x: u32, y: u32| {
        let index = usize::try_from((y * 24 + x) * 4).unwrap();
        frame.pixels[index..index + 3].to_vec()
    };

    assert_eq!(at(0, 0), [10, 20, 30]);
    assert!(at(12, 12).iter().all(|channel| *channel < 10));
    assert!(at(3, 3).iter().all(|channel| *channel > 100));
}

#[test]
#[ignore = "writes ray traced black holes as PNGs into $OX_RODEO_SHOTS"]
fn black_hole_shots() {
    let Ok(folder) = std::env::var("OX_RODEO_SHOTS") else {
        return;
    };
    let folder = PathBuf::from(folder);
    std::fs::create_dir_all(&folder).unwrap();
    let disk = Some(Disk::default());
    let shots = [
        (
            "sky",
            Scene {
                lensing: false,
                ..Scene::default()
            },
        ),
        ("lensed-sky", Scene::default()),
        (
            "flat-disk",
            Scene {
                lensing: false,
                disk,
                ..Scene::default()
            },
        ),
        (
            "edge-on",
            Scene {
                disk,
                ..Scene::default()
            },
        ),
        (
            "tilted",
            Scene {
                disk,
                tilt: 0.6,
                ..Scene::default()
            },
        ),
        (
            "face-on",
            Scene {
                disk,
                tilt: 1.45,
                ..Scene::default()
            },
        ),
        (
            "star",
            Scene {
                star: Some(Star {
                    radius: 6.0,
                    ..Star::default()
                }),
                ..Scene::default()
            },
        ),
        (
            "collapsing",
            Scene {
                star: Some(Star {
                    radius: 1.2,
                    ..Star::default()
                }),
                ..Scene::default()
            },
        ),
    ];
    for (name, scene) in shots {
        let (width, height) = (640, 360);
        let started = std::time::Instant::now();
        let pixels = render(
            &Scene {
                samples: 2,
                ..scene
            },
            width,
            height,
        );
        println!("{name}: {:?}", started.elapsed());
        let bytes: Vec<u8> = pixels.iter().flat_map(|color| color.to_rgba8()).collect();
        image::save_buffer(
            folder.join(format!("{name}.png")),
            &bytes,
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();
    }
}

#[test]
fn frames_drawn_from_the_sky_cache_match_fresh_ones() {
    let scene = |radius| Scene {
        star: Some(Star {
            radius,
            ..Star::default()
        }),
        distance: 40.0,
        fov: 0.8,
        ..Scene::default()
    };
    let other = Scene {
        tilt: 0.7,
        ..Scene::default()
    };

    render(&scene(6.0), 48, 32);
    let cached = render(&scene(1.5), 48, 32);
    render(&other, 48, 32);
    let fresh = render(&scene(1.5), 48, 32);

    assert_eq!(cached, fresh);
}

#[test]
#[ignore = "times the frames of a turning disk under a still camera"]
fn turning_disk_timing() {
    let scene = |time| Scene {
        disk: Some(Disk::default()),
        time,
        ..Scene::default()
    };
    for (samples, width, height) in [(1, 640, 360), (2, 640, 360), (1, 960, 540)] {
        render(
            &Scene {
                samples,
                ..scene(0.0)
            },
            width,
            height,
        );
        let started = std::time::Instant::now();
        for frame in 1..=5 {
            render(
                &Scene {
                    samples,
                    ..scene(f64::from(frame))
                },
                width,
                height,
            );
        }
        println!(
            "{width}x{height} x{samples}: {:?} a frame",
            started.elapsed() / 5
        );
    }
}
