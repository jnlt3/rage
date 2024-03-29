use crate::r_core::env::{Env, Team};
use std::sync::{Arc, Mutex};
use crate::r_core::state::{Bullet, State, BotData};
use serde::{Serialize, Deserialize};
use std::collections::VecDeque;
use std::time::Instant;
use crate::r_core::math::vec2::Vec2;
use crate::r_core::map::map::Map;
use std::ops::Index;
use std::borrow::Borrow;
use crate::r_core::map::map::Shape::RotRect;

const PI: f32 = std::f32::consts::PI;

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct OutputAction {
    m: i32,
    r: i32,
    f: i32,
}

impl OutputAction {
    pub fn new() -> Self {
        Self {
            m: 0,
            r: 0,
            f: 0,
        }
    }
}

pub struct Controller {
    env: Arc<Mutex<Env>>,
    out_action: Arc<Mutex<OutputAction>>,
    map: Arc<Map>,
    controller: MinimumDangerController,
    team: Team,
}

impl Controller {
    pub fn new(env: Arc<Mutex<Env>>, map: Arc<Map>, team: Team) -> Self {
        Self {
            env,
            map: map.clone(),
            out_action: Arc::new(Mutex::new(OutputAction::new())),
            controller: MinimumDangerController::new(map),
            team,
        }
    }

    pub fn out_data(&self) -> OutputAction {
        *self.out_action.lock().unwrap()
    }

    pub fn update(&mut self) {
        let env = &mut *self.env.lock().unwrap();
        let state = env.current_state();
        if state.is_none() {
            return;
        }
        let state = state.unwrap();
        let bullet_0 = env.get_bot_bullet(self.team);
        let bullet_1 = env.get_enemy_bullet(self.team);
        let action = self.controller.action(
            env.last_update().elapsed().as_secs_f32(),
            state,
            bullet_0,
            bullet_1,
        );
        let out_action = &mut *self.out_action.lock().unwrap();
        out_action.m = action.0 as i32;
        out_action.r = -action.1 as i32;
        out_action.f = if action.2 { 1 } else { 0 };
    }
}

const ANGLE_CNT: usize = 45;
const PROJECT_DIST: f32 = 200f32;

pub struct MinimumDangerController {
    angles: [Vec2; ANGLE_CNT],
    points: [Vec2; ANGLE_CNT],
    max_size: usize,
    map: Arc<Map>,

    counter: Instant,
}

impl MinimumDangerController {
    pub fn new(map: Arc<Map>) -> Self {
        let mut angles = [Vec2::new(0f32, 0f32); ANGLE_CNT];
        for (index, angle) in angles.iter_mut().enumerate() {
            *angle = Vec2::from_angle(Self::angle(index)) * PROJECT_DIST;
        }
        Self {
            map,
            angles,
            points: [Vec2::new(0f32, 0f32); ANGLE_CNT],
            max_size: 10000,
            counter: Instant::now(),
        }
    }

    pub fn angle(index: usize) -> f32 {
        (index as f32 / ANGLE_CNT as f32 * PI * 2f32)
    }

    pub fn relative_angle(mut angle: f32) -> f32 {
        angle %= PI * 2f32;
        if angle >= 0f32 {
            if angle < PI {
                angle
            } else {
                angle - PI * 2f32
            }
        } else {
            if angle >= -PI {
                angle
            } else {
                angle + PI * 2f32
            }
        }
    }

    pub fn bullet_check(&self, angle: f32, state: State) -> bool {
        let bot = RotRect {
            pos: state.bot.pos,
            w_h: Vec2::new(79.6129 * 1.2f32, 124.67 * 1.2f32),
            rot: state.bot.r,
        };

        let enemy = RotRect {
            pos: state.opp.pos,
            w_h: Vec2::new(79.6129, 124.67),
            rot: state.opp.r,
        };
        let cos_sin = Vec2::from_angle(angle);
        let trajectory = self.map.get_bullet_trajectory(
            Bullet::new(
                state.bot.pos.x(),
                state.bot.pos.y(),
                cos_sin.x() * 400f32,
                cos_sin.y() * 400f32,
            ),
            1f32,
            100,
        );
        let mut start = false;
        for i in 0..602 {
            let time = i as f32 / 100f32;
            let pos = trajectory.position_in(time);
            if let Some(pos) = pos {
                if !bot.intersects(pos) {
                    start = true;
                } else if start {
                    return false;
                }
                if enemy.intersects(pos) {
                    return true;
                }
            } else {
                return false;
            }
        }
        return false;
    }

    pub fn action(&mut self, delta_time: f32, state: State, bullet_0: Option<Bullet>, bullet_1: Option<Bullet>) -> (f32, f32, bool) {
        let mut lowest_danger = f32::INFINITY;
        let mut target = 0;


        let mut check_bullet = vec![];
        if let Some(b_0) = bullet_0 {
            let trajectory = self.map.get_bullet_trajectory(
                b_0,
                1f32,
                100,
            );
            for i in 0..10 {
                let time = i as f32 * 0.1;
                if let Some(pos) = trajectory.position_in(time) {
                    check_bullet.push(pos)
                }
            }
        }
        if let Some(b_1) = bullet_1 {
            let trajectory = self.map.get_bullet_trajectory(
                b_1,
                1f32,
                100,
            );
            for i in 0..10 {
                let time = i as f32 * 0.1;
                if let Some(pos) = trajectory.position_in(time) {
                    check_bullet.push(pos)
                }
            }
        }

        let mut dodge = false;
        let mut shoot = false;
        for (index, p_pos) in self.angles.iter().enumerate() {
            let pos = state.bot.pos + *p_pos;
            let mut danger = 0f32;
            //danger += (pos - state.opp.pos).sq_magnitude().sqrt();
            //println!("{}", danger);

            if !self.map.intersects(pos) {
                for &b_0 in &check_bullet {
                    danger += 5000f32 / (1f32 + (b_0 - pos).sq_magnitude().sqrt());
                    dodge = true;
                }
                let rc = self.map.ray_cast(state.bot.pos, &[Self::angle(index)], 1f32, 0..200)[0];
                let dist = (pos - rc.1).sq_magnitude().sqrt();
                danger += 1f32 / (1f32 + dist);
                let dist = (pos - state.opp.pos).sq_magnitude().sqrt();
                danger -= 100f32 / (1f32 + (dist.powi(2)));
                if !dodge && self.bullet_check(Self::angle(index), state) {
                    danger -= 1f32 / (1f32 + (Self::angle(index) - state.abs_bearing).abs());
                    shoot = true;
                }

                let diff = Self::angle(index) - state.abs_bearing;
                danger -= 100f32 / (1f32 + Self::relative_angle(diff).powi(2));
                if danger < lowest_danger {
                    lowest_danger = danger;
                    target = index;
                }
            }
        }


        let mut turn_amt = Self::relative_angle(-Self::angle(target) - state.bot.r);

        let mut move_dir = 1f32;
        if dodge && turn_amt.abs() > std::f32::consts::FRAC_PI_2 {
            move_dir *= -1f32;
            if turn_amt > std::f32::consts::FRAC_PI_2 {
                turn_amt -= std::f32::consts::PI;
            } else {
                turn_amt += std::f32::consts::PI;
            }
        }
        let final_shoot = turn_amt.abs() < (100f32 / state.dist) && shoot && state.bot.can_fire;
        let turn_amt = if turn_amt.abs() < (100f32 / state.dist) || final_shoot {
            0f32
        } else {
            turn_amt.signum()
        };
        (move_dir, turn_amt, final_shoot)
    }
}