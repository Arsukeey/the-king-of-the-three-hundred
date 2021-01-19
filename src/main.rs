#[allow(dead_code)]

pub mod camera;
pub mod hittable;
pub mod material;
pub mod ray;
pub mod sphere;
pub mod vec3;
pub mod aabb;
pub mod bvh;
pub mod texture;
pub mod perlin;
pub mod aarect;
pub mod onb;
pub mod pdf;
pub mod scenes;

use hittable::*;
use material::*;
use ray::Ray;
use vec3::*;
use pdf::*;

use std::mem;
use std::ptr;

use std::sync::{Arc, Mutex};
use rayon::prelude::*;

use rand::seq::SliceRandom;

const ASPECT_RATIO: f32 = 1.0;
const NX: usize = 500;
const NY: usize = (NX as f32 / ASPECT_RATIO) as usize;
const SAMPLES_PER_PIXEL: usize = 1000;
const MAX_DEPTH: i32 = 50;

// assumes constructor will never panic. we're safe using just Box::new()
macro_rules! make_array {
    ($constructor:expr; $n:expr) => {{
        let mut items: [_; $n] = mem::MaybeUninit::uninit().assume_init();
        for place in items.iter_mut() {
            ptr::write(place, $constructor);
        }
        items
    }}
}

fn main() {
    println!("P3\n{} {}\n255", NX, NY);

    let (world, cam, background, lights) = scenes::cornell_box(ASPECT_RATIO);

    eprintln!("Rendering!");
    let image = unsafe { Arc::new(Mutex::new(
        Box::new(make_array!( Box::new([Vec3::new_empty(); NX]); NY ),
    ))) };

    // deterministic and low-discrepancy sequence for MC sims
    let hx = halton::Sequence::new(2).map(|x| x as f32).take(SAMPLES_PER_PIXEL).collect::<Vec<f32>>();
    let hy = halton::Sequence::new(3).map(|x| x as f32).take(SAMPLES_PER_PIXEL).collect::<Vec<f32>>();

    (0..NY).into_par_iter().rev().for_each(|y| {
        eprintln!("Scanlines remaining: {}", y);
        for x in 0..NX {
            let mut pixel_color = Color::new(0.0, 0.0, 0.0);

            for i in 0..SAMPLES_PER_PIXEL {
                let u = (x as f32 + hx[i]) / (NX - 1) as f32;
                let v = (y as f32 + hy[i]) / (NY - 1) as f32;

                let r = cam.get_ray(u, v);
                pixel_color += ray_color(r, background, &world, &lights, MAX_DEPTH);
            }

            image.lock().unwrap()[y as usize][x as usize] =
                Vec3::calc_color(pixel_color, SAMPLES_PER_PIXEL);
        }
    });

    eprintln!("Outputting image!");
    let img = image.lock().unwrap();
    for y in (0..img.len()).rev() {
        for x in 0..img[y].len() {
            println!(
                "{} {} {}",
                img[y][x].x as u8, img[y][x].y as u8, img[y][x].z as u8
            );
        }
    }
}

fn ray_color(ray: Ray, background: Color, world: &HittableList, lights: &HittableList, depth: i32) -> Color {
    if depth <= 0 {
        return Color::new_empty();
    }

    match world.hit(&ray, 0.001, std::f32::INFINITY) {
        Some(hit) => {
            let emitted = hit.material.emitted(ray.clone(), &hit);

            if let Some(reflection) = hit.material.scatter(ray.clone(), &hit) {
                match reflection {
                    ReflectionRecord::Specular { specular_ray, attenuation } => {
                        return attenuation *
                            ray_color(specular_ray, background, world, &lights, depth - 1);
                    }

                    ReflectionRecord::Scatter { pdf: reflection_cosine_pdf, attenuation } => {
                        let light_obj_pdf = if lights.len() == 1 {
                            lights.first()
                        } else {
                            let mut rng = rand::thread_rng();
                            lights.objects.choose(&mut rng)
                        };
                        
                        let pdf: Box<dyn PDF> = if let Some(&hittable) = light_obj_pdf {
                            let light_pdf = HittablePDF::new(hit.p, hittable);
                            Box::new(MixturePDF::new(light_pdf, reflection_cosine_pdf))
                        } else {
                            // no lights, so no importance sampling
                            Box::new(CosinePDF::new(hit.normal))
                        };
                        
                        let scattered = Ray::new(hit.p, pdf.generate(), ray.time);
                        let pdf_val = pdf.value(scattered.dir);

                        return emitted + attenuation
                            * hit.material.scattering_pdf(ray, &hit, scattered.clone())
                            * ray_color(scattered, background, world, &lights, depth - 1) / pdf_val
                    }
                }
            }

            return emitted;
        }
        None => {
            return background;
        }
    }
}
