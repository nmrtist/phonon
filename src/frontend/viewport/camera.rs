use eframe::egui::{Pos2, Rect, Vec2};
use nalgebra::{Point3, Vector3};

use crate::domain::Structure;

#[derive(Clone, Copy, Default, PartialEq)]
pub struct ViewCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub pan: Vec2,
    pub zoom: f32,
}

pub(super) struct Projector {
    pub(super) rect: Rect,
    pub(super) center: Point3<f32>,
    pub(super) scale: f32,
    pub(super) camera_distance: f32,
    pub(super) yaw: f32,
    pub(super) pitch: f32,
    pub(super) pan: Vec2,
}

#[derive(Clone, Copy)]
pub(super) struct Projected {
    pub(super) pos: Pos2,
    pub(super) depth: f32,
    pub(super) scale: f32,
}

impl Projector {
    pub(super) fn view_space(&self, point: Point3<f32>) -> Vector3<f32> {
        rotate(point - self.center, self.yaw, self.pitch)
    }

    pub(super) fn project(&self, point: Point3<f32>) -> Projected {
        let rotated = self.view_space(point);
        let near_plane = (self.camera_distance * 0.2).max(0.1);
        let perspective = self.camera_distance / (self.camera_distance - rotated.z).max(near_plane);
        let screen_center = self.rect.center() + self.pan;

        Projected {
            pos: Pos2::new(
                screen_center.x + rotated.x * self.scale * perspective,
                screen_center.y - rotated.y * self.scale * perspective,
            ),
            depth: rotated.z,
            scale: perspective,
        }
    }
}

pub(super) fn view_center_and_radius(
    structure: &Structure,
    include_cell: bool,
) -> (Point3<f32>, f32) {
    if include_cell {
        return (structure.center(), structure.radius().max(1.0));
    }
    if structure.atoms.is_empty() {
        return (Point3::origin(), 1.0);
    }

    let sum = structure
        .atoms
        .iter()
        .fold(Vector3::zeros(), |acc, atom| acc + atom.position.coords);
    let center = Point3::from(sum / structure.atoms.len() as f32);
    let radius = structure
        .atoms
        .iter()
        .map(|atom| nalgebra::distance(&center, &atom.position))
        .fold(1.0_f32, f32::max);
    (center, radius)
}

pub(super) fn rotate(v: Vector3<f32>, yaw: f32, pitch: f32) -> Vector3<f32> {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let x = cy * v.x + sy * v.z;
    let z = -sy * v.x + cy * v.z;
    let y = cp * v.y - sp * z;
    let z = sp * v.y + cp * z;

    Vector3::new(x, y, z)
}

pub(super) fn inverse_rotate(v: Vector3<f32>, yaw: f32, pitch: f32) -> Vector3<f32> {
    let (sy, cy) = yaw.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    let y = cp * v.y + sp * v.z;
    let z = -sp * v.y + cp * v.z;
    let x = cy * v.x - sy * z;
    let z = sy * v.x + cy * z;

    Vector3::new(x, y, z)
}

pub(super) fn camera_forward_world(viewport: &Projector) -> Vector3<f32> {
    inverse_rotate(Vector3::new(0.0, 0.0, 1.0), viewport.yaw, viewport.pitch)
        .try_normalize(0.000001)
        .unwrap_or(Vector3::new(0.0, 0.0, 1.0))
}
