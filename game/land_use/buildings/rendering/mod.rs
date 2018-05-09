use descartes::Circle;
use compact::{CVec, CDict};
use kay::{ActorSystem, World, External, TypedID, Actor};
use monet::{RendererID, Renderable, RenderableID, GrouperID, GrouperIndividualID, Geometry,
            Instance};
use stagemaster::geometry::AnyShape;
use stagemaster::{UserInterface, UserInterfaceID, Event3d, Interactable3d, Interactable3dID,
                  Interactable2d, Interactable2dID};
use imgui::ImGuiSetCond_FirstUseEver;

use super::{Building, Lot, BuildingID, BuildingPlanResultDelta, BuildingStyle};
use economy::households::HouseholdID;
use style::colors;

use super::architecture::{BuildingGeometry, build_building};

#[derive(Compact, Clone)]
pub struct BuildingInspector {
    id: BuildingInspectorID,
    user_interface: UserInterfaceID,
    current_building: Option<BuildingID>,
    current_households: CVec<HouseholdID>,
    households_todo: CVec<HouseholdID>,
    return_ui_to: Option<UserInterfaceID>,
}

impl BuildingInspector {
    pub fn spawn(
        id: BuildingInspectorID,
        user_interface: UserInterfaceID,
        _: &mut World,
    ) -> BuildingInspector {
        BuildingInspector {
            id,
            user_interface,
            current_building: None,
            current_households: CVec::new(),
            households_todo: CVec::new(),
            return_ui_to: None,
        }
    }

    pub fn set_inspected_building(
        &mut self,
        building: BuildingID,
        households: &CVec<HouseholdID>,
        world: &mut World,
    ) {
        self.current_building = Some(building);
        self.current_households = households.clone();
        self.households_todo.clear();
        self.user_interface.add_2d(self.id_as(), world);
    }

    pub fn ui_drawn(&mut self, imgui_ui: &External<::imgui::Ui<'static>>, world: &mut World) {
        let ui = imgui_ui.steal();

        if let Some(household) = self.households_todo.pop() {
            household.inspect(ui, self.id, world);
        } else {
            self.return_ui_to
                .expect("Should have return to set for UI")
                .ui_drawn(ui, world);
        }
    }
}

impl Interactable2d for BuildingInspector {
    fn draw_ui_2d(
        &mut self,
        imgui_ui: &External<::imgui::Ui<'static>>,
        return_to: UserInterfaceID,
        world: &mut World,
    ) {
        let ui = imgui_ui.steal();
        self.return_ui_to = Some(return_to);

        let new_building = if let Some(building) = self.current_building {
            let mut opened = true;

            ui.window(im_str!("Building"))
                .size((230.0, 400.0), ImGuiSetCond_FirstUseEver)
                .position((10.0, 220.0), ImGuiSetCond_FirstUseEver)
                .collapsible(false)
                .opened(&mut opened)
                .build(|| {
                    ui.text(im_str!("Building RawID: {:?}", building.as_raw()));
                    ui.text(im_str!(
                        "# of households: {}",
                        self.current_households.len()
                    ))
                });

            self.households_todo = self.current_households.clone();
            self.return_ui_to = Some(return_to);
            self.id.ui_drawn(ui, world);

            if opened { Some(building) } else { None }
        } else {
            return_to.ui_drawn(ui, world);
            None
        };

        self.current_building = new_building;
    }
}

impl Interactable3d for Building {
    fn on_event(&mut self, event: Event3d, world: &mut World) {
        if let Event3d::DragFinished { .. } = event {
            BuildingInspector::local_first(world)
                .set_inspected_building(self.id, self.all_households().into(), world);
        };
    }
}

#[derive(Compact, Clone)]
pub struct BuildingRenderer {
    id: BuildingRendererID,
    wall_grouper: GrouperID,
    flat_roof_grouper: GrouperID,
    brick_roof_grouper: GrouperID,
    field_grouper: GrouperID,
    current_n_buildings_to_be_destroyed: CDict<RendererID, usize>,
}

impl BuildingRenderer {
    pub fn spawn(id: BuildingRendererID, world: &mut World) -> BuildingRenderer {
        BuildingRenderer {
            id,
            wall_grouper: GrouperID::spawn(colors::WALL, 5000, false, world),
            flat_roof_grouper: GrouperID::spawn(colors::FLAT_ROOF, 5100, false, world),
            brick_roof_grouper: GrouperID::spawn(colors::BRICK_ROOF, 5200, false, world),
            field_grouper: GrouperID::spawn(colors::FIELD, 5300, false, world),
            current_n_buildings_to_be_destroyed: CDict::new(),
        }
    }

    pub fn add_geometry(&mut self, id: BuildingID, geometry: &BuildingGeometry, world: &mut World) {
        // TODO: ugly: Building is not really a GrouperIndividual
        self.wall_grouper.add_frozen(
            unsafe { GrouperIndividualID::from_raw(id.as_raw()) },
            geometry.wall.clone(),
            world,
        );
        self.flat_roof_grouper.add_frozen(
            unsafe { GrouperIndividualID::from_raw(id.as_raw()) },
            geometry.flat_roof.clone(),
            world,
        );
        self.brick_roof_grouper.add_frozen(
            unsafe { GrouperIndividualID::from_raw(id.as_raw()) },
            geometry.brick_roof.clone(),
            world,
        );
        self.field_grouper.add_frozen(
            unsafe { GrouperIndividualID::from_raw(id.as_raw()) },
            geometry.field.clone(),
            world,
        );
    }

    pub fn remove_geometry(&mut self, building_id: BuildingID, world: &mut World) {
        self.wall_grouper.remove(
            unsafe { GrouperIndividualID::from_raw(building_id.as_raw()) },
            world,
        );
        self.flat_roof_grouper.remove(
            unsafe { GrouperIndividualID::from_raw(building_id.as_raw()) },
            world,
        );
        self.brick_roof_grouper.remove(
            unsafe { GrouperIndividualID::from_raw(building_id.as_raw()) },
            world,
        );
        self.field_grouper.remove(
            unsafe { GrouperIndividualID::from_raw(building_id.as_raw()) },
            world,
        );
    }

    pub fn update_buildings_to_be_destroyed(
        &mut self,
        renderer_id: RendererID,
        building_plan_result_delta: &BuildingPlanResultDelta,
        world: &mut World,
    ) {
        let new_buildings_to_be_destroyed = &building_plan_result_delta.buildings_to_destroy;
        let existing_n_to_be_destroyed = self.current_n_buildings_to_be_destroyed
            .get(renderer_id)
            .cloned()
            .unwrap_or(0);
        for i in new_buildings_to_be_destroyed.len()..existing_n_to_be_destroyed {
            renderer_id.update_individual(
                37_000 + i as u16,
                Geometry::empty(),
                Instance::with_color([1.0, 0.0, 0.0]),
                true,
                world,
            );
        }

        for (i, building) in new_buildings_to_be_destroyed.iter().enumerate() {
            building.render_as_destroyed(renderer_id, i, world);
        }

        self.current_n_buildings_to_be_destroyed.insert(
            renderer_id,
            new_buildings_to_be_destroyed.len(),
        );
    }
}

impl Renderable for BuildingRenderer {
    fn setup_in_scene(&mut self, renderer_id: RendererID, world: &mut World) {
        // let band_path = CPath::new(vec![
        //     Segment::arc_with_direction(
        //         P2::new(5.0, 0.0),
        //         V2::new(0.0, 1.0),
        //         P2::new(-5.0, 0.0)
        //     ),
        //     Segment::arc_with_direction(
        //         P2::new(-5.0, 0.0),
        //         V2::new(0.0, -1.0),
        //         P2::new(5.0, 0.0)
        //     ),
        // ]);
        // let building_circle = band_to_geometry(&Band::new(band_path, 2.0), 0.0);
        // renderer_id.add_batch(11_111, building_circle, world);
        Into::<RenderableID>::into(self.wall_grouper).setup_in_scene(renderer_id, world);
        Into::<RenderableID>::into(self.flat_roof_grouper).setup_in_scene(renderer_id, world);
        Into::<RenderableID>::into(self.brick_roof_grouper).setup_in_scene(renderer_id, world);
        Into::<RenderableID>::into(self.field_grouper).setup_in_scene(renderer_id, world);
    }

    fn render_to_scene(&mut self, renderer_id: RendererID, frame: usize, world: &mut World) {
        // let renderable_buildings: RenderableID = BuildingID::local_broadcast(world).into();
        // renderable_buildings.render_to_scene(renderer_id, frame, world);
        Into::<RenderableID>::into(self.wall_grouper).render_to_scene(renderer_id, frame, world);
        Into::<RenderableID>::into(self.flat_roof_grouper)
            .render_to_scene(renderer_id, frame, world);
        Into::<RenderableID>::into(self.brick_roof_grouper)
            .render_to_scene(renderer_id, frame, world);
        Into::<RenderableID>::into(self.field_grouper).render_to_scene(renderer_id, frame, world);
    }
}

// impl Renderable for Building {
//     fn setup_in_scene(&mut self, _renderer_id: RendererID, _scene_id: usize, _: &mut World) {}

//     fn render_to_scene(
//         &mut self,
//         renderer_id: RendererID,
//         scene_id: usize,
//         frame: usize,
//         world: &mut World,
//     ) {
//         // TODO: this is super hacky
//         let is_shop = self.households[0].as_raw().local_broadcast() ==
//             GroceryShopID::local_broadcast(world).as_raw();
//         renderer_id.add_instance(
//             scene_id,
//             11_111,
//             frame,
//             Instance {
//                 instance_position: [self.lot.position.x, self.lot.position.y, 0.0],
//                 instance_direction: [1.0, 0.0],
//                 instance_color: if is_shop {
//                     [0.2, 0.2, 0.8]
//                 } else {
//                     [0.3, 0.8, 0.0]
//                 },
//             },
//             world,
//         );
//     }
// }

pub fn setup(system: &mut ActorSystem, user_interface: UserInterfaceID) -> BuildingRendererID {
    system.register::<BuildingInspector>();
    system.register::<BuildingRenderer>();
    auto_setup(system);

    BuildingInspectorID::spawn(user_interface, &mut system.world());

    BuildingRendererID::spawn(&mut system.world())
}

use util::random::seed;

pub fn on_add(id: BuildingID, lot: &Lot, building_type: BuildingStyle, world: &mut World) {
    // TODO: not sure if correct
    UserInterface::local_first(world).add(
        ::ui_layers::UILayer::Info as usize,
        id.into(),
        AnyShape::Circle(
            Circle { center: lot.center_point, radius: 5.0 },
        ),
        10,
        world,
    );

    BuildingRenderer::local_first(world).add_geometry(
        id,
        build_building(
            lot,
            building_type,
            &mut seed(id),
        ),
        world,
    )
}

pub fn on_destroy(building_id: BuildingID, world: &mut World) {
    UserInterface::local_first(world).remove(
        ::ui_layers::UILayer::Info as usize,
        building_id.into(),
        world,
    );
    BuildingRenderer::local_first(world).remove_geometry(building_id, world);
}

impl Building {
    pub fn render_as_destroyed(
        &mut self,
        renderer_id: RendererID,
        building_index: usize,
        world: &mut World,
    ) {
        let geometries = build_building(&self.lot, self.style, &mut seed(self.id));

        let combined_geometry = geometries.brick_roof + geometries.flat_roof + geometries.wall +
            geometries.field;

        renderer_id.update_individual(
            37_000 + building_index as u16,
            combined_geometry,
            Instance::with_color([1.0, 0.0, 0.0]),
            true,
            world,
        );
    }
}

mod kay_auto;
pub use self::kay_auto::*;
