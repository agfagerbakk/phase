//! Peter Parker's Camera — "This artifact enters with three film counters on
//! it.\n{2}, {T}, Remove a film counter from this artifact: Copy target
//! activated or triggered ability you control. You may choose new targets for
//! the copy."
//!
//! CR 602.1 + CR 602.2: the ability may be activated any time its costs can be
//! paid. Three film counters fund three activations, one per untap (CR 502.3),
//! so the Camera must remain activatable on later turns until the counters run
//! out.

use engine::ai_support::legal_actions;
use engine::game::scenario::{GameRunner, GameScenario, P0, P1};
use engine::game::scenario_db::GameScenarioDbExt;
use engine::types::ability::TargetRef;
use engine::types::actions::GameAction;
use engine::types::counter::CounterType;
use engine::types::game_state::{CastPaymentMode, WaitingFor};
use engine::types::identifiers::ObjectId;
use engine::types::mana::{ManaCost, ManaType, ManaUnit};
use engine::types::phase::Phase;
use engine::types::zones::Zone;

use crate::support::shared_card_db as load_db;

const CAMERA: &str = "This artifact enters with three film counters on it.\n{2}, {T}, Remove a film counter from this artifact: Copy target activated or triggered ability you control. You may choose new targets for the copy.";
/// A plain activated ability for the Camera to copy.
const BATTERY: &str = "{1}: You gain 1 life.";

fn film(runner: &GameRunner, camera: ObjectId) -> u32 {
    runner.state().objects[&camera]
        .counters
        .get(&CounterType::Generic("film".to_string()))
        .copied()
        .unwrap_or(0)
}

fn add_colorless(runner: &mut GameRunner, amount: usize) {
    for _ in 0..amount {
        runner.state_mut().players[0].mana_pool.add(ManaUnit::new(
            ManaType::Colorless,
            ObjectId(0),
            false,
            vec![],
        ));
    }
}

/// Pass turns until P0 again has priority in its precombat main phase.
fn advance_to_next_p0_main(runner: &mut GameRunner) {
    for _ in 0..4 {
        runner.advance_to_phase(Phase::End);
        runner.advance_to_phase(Phase::PreCombatMain);
        if runner.state().active_player == P0 {
            return;
        }
    }
    panic!("never returned to P0's precombat main");
}

/// Activate the battery, then the Camera targeting the battery's ability, and
/// resolve the stack.
fn copy_battery_with_camera(runner: &mut GameRunner, camera: ObjectId, battery: ObjectId) {
    add_colorless(runner, 3);
    runner
        .act(GameAction::ActivateAbility {
            source_id: battery,
            ability_index: 0,
            payment_mode: CastPaymentMode::Auto,
        })
        .expect("battery's {1} ability must activate");
    let battery_entry = runner.state().stack.back().expect("battery on stack").id;

    // CR 117.3c: the activating player receives priority with the battery's
    // ability on the stack. The client only offers what `legal_actions` lists,
    // so the Camera must be among them — not merely accepted by `act`.
    assert!(
        legal_actions(runner.state()).iter().any(|action| matches!(
            action,
            GameAction::ActivateAbility { source_id, .. } if *source_id == camera
        )),
        "the Camera must be offered as a legal action while it has film counters",
    );

    runner
        .act(GameAction::ActivateAbility {
            source_id: camera,
            ability_index: 0,
            payment_mode: CastPaymentMode::Auto,
        })
        .expect("Peter Parker's Camera must be activatable while it has film counters");
    if matches!(
        runner.state().waiting_for,
        WaitingFor::TargetSelection { .. }
    ) {
        runner
            .act(GameAction::SelectTargets {
                targets: vec![TargetRef::Object(battery_entry)],
            })
            .expect("the battery's ability is a legal target");
    }
    runner.advance_until_stack_empty();
}

#[test]
fn peter_parkers_camera_activates_once_per_film_counter_across_turns() {
    let Some(db) = load_db() else {
        return;
    };
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    let camera = scenario
        .add_artifact_to_hand_from_oracle(P0, "Peter Parker's Camera", CAMERA)
        .with_mana_cost(ManaCost::zero())
        .id();
    let battery = scenario
        .add_artifact_from_oracle(P0, "Battery", BATTERY)
        .id();
    // Libraries so neither player decks out while turns pass.
    for player in [P0, P1] {
        for name in ["Forest", "Island", "Mountain", "Plains", "Swamp", "Forest"] {
            scenario.add_real_card(player, name, Zone::Library, db);
        }
    }
    let mut runner = scenario.build();

    let card_id = runner.state().objects[&camera].card_id;
    runner
        .act(GameAction::CastSpell {
            object_id: camera,
            card_id,
            targets: vec![],
            payment_mode: CastPaymentMode::Auto,
        })
        .expect("casting the Camera should succeed");
    runner.advance_until_stack_empty();
    assert_eq!(
        runner.state().objects[&camera].zone,
        Zone::Battlefield,
        "the Camera resolves onto the battlefield",
    );
    assert_eq!(
        film(&runner, camera),
        3,
        "the Camera enters with three film counters"
    );

    for turn in 0..3u32 {
        assert!(
            !runner.state().objects[&camera].tapped,
            "turn {turn}: the Camera untaps during its controller's untap step",
        );
        let life_before = runner.state().players[0].life;
        copy_battery_with_camera(&mut runner, camera, battery);
        assert_eq!(
            film(&runner, camera),
            2 - turn,
            "turn {turn}: each activation removes exactly one film counter",
        );
        assert_eq!(
            runner.state().players[0].life,
            life_before + 2,
            "turn {turn}: the battery's ability and its copy both resolve",
        );
        assert!(runner.state().objects[&camera].tapped);
        advance_to_next_p0_main(&mut runner);
    }

    // CR 602.2b + CR 118.3: with no film counter left the cost can't be paid.
    add_colorless(&mut runner, 3);
    runner
        .act(GameAction::ActivateAbility {
            source_id: battery,
            ability_index: 0,
            payment_mode: CastPaymentMode::Auto,
        })
        .expect("battery's {1} ability must activate");
    assert!(
        runner
            .act(GameAction::ActivateAbility {
                source_id: camera,
                ability_index: 0,
                payment_mode: CastPaymentMode::Auto,
            })
            .is_err()
            || film(&runner, camera) == 0 && runner.state().stack.len() == 1,
        "the Camera can't be activated once its film counters are gone",
    );
}
