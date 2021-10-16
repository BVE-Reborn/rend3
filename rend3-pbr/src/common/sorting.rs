use std::borrow::Cow;

use ordered_float::OrderedFloat;
use rend3::resources::{CameraManager, InternalObject};

use crate::culling::Sorting;

pub fn sort_objects<'a>(
    objects: &'a [InternalObject],
    camera_manager: &CameraManager,
    sorting: Option<Sorting>,
) -> Cow<'a, [InternalObject]> {
    if let Some(sorting) = sorting {
        profiling::scope!("Sorting");

        let camera_location = camera_manager.location().into();

        let sorted_objects = objects.to_vec();

        match sorting {
            Sorting::FrontToBack => {
                sorted_objects
                    .sort_unstable_by_key(|o| OrderedFloat(o.mesh_location().distance_squared(camera_location)));
            }
            Sorting::BackToFront => {
                sorted_objects
                    .sort_unstable_by_key(|o| OrderedFloat(-o.mesh_location().distance_squared(camera_location)));
            }
        }

        Cow::Owned(sorted_objects)
    } else {
        Cow::Borrowed(objects)
    }
}
