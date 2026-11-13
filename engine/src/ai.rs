//! Rival controlado por la máquina: decide cada pocos ticks qué hacer y lo
//! expresa activando comandos, igual que si los hubiera tecleado.

use crate::game::Game;

#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub enum Plan {
    #[default]
    Idle,
    Forward,
    Back,
    Guard,
    CrouchGuard,
    Crouch,
    Jump,
    JumpForward,
}

#[derive(Clone, Default)]
pub struct AiMem {
    pub plan: Plan,
    pub timer: i32,
    /// Comando de un solo tick pendiente de lanzar.
    pub press: Option<String>,
}

pub fn think(g: &mut Game, c: usize) {
    if g.round.state != 2 || g.chars[c].life <= 0 {
        return;
    }
    let o = g.enemy(c);
    let dist = g.body_dist(c);
    let enemy_attacking = g.chars[o].movetype == b'a';
    let enemy_low = g.chars[o].stype == b'c';
    let enemy_down = g.chars[o].stype == b'l' || g.chars[o].life <= 0;
    let in_air = g.chars[c].stype == b'a';

    let mut mem = std::mem::take(&mut g.chars[c].ai_mem);
    mem.timer -= 1;
    if mem.timer <= 0 {
        let r = g.rand();
        let pick = |list: &[&str], r: i32| list[(r as usize / 7) % list.len()].to_string();
        mem.press = None;
        if enemy_attacking && dist < 90.0 && r < 550 {
            mem.plan = if enemy_low { Plan::CrouchGuard } else { Plan::Guard };
            mem.timer = 18;
        } else if enemy_down {
            mem.plan = if dist < 40.0 { Plan::Back } else { Plan::Idle };
            mem.timer = 15;
        } else if in_air {
            mem.plan = Plan::Idle;
            mem.timer = 8;
            if dist < 70.0 && r < 600 {
                mem.press = Some(pick(&["a", "b", "x", "y", "z", "c"], g.rand()));
            }
        } else if dist > 70.0 {
            (mem.plan, mem.timer) = match r {
                0..=549 => (Plan::Forward, 14 + r % 20),
                550..=699 => (Plan::JumpForward, 4),
                700..=779 => (Plan::Idle, 10),
                780..=879 => (Plan::Back, 12),
                _ => (Plan::Idle, 6),
            };
            if r >= 880 {
                // Algún especial a distancia.
                let specials: Vec<String> = g.chars[c].def.cmds.cmds.iter().filter(|k| k.is_motion()).map(|k| k.name.clone()).collect();
                if !specials.is_empty() {
                    mem.press = Some(specials[(g.rand() as usize) % specials.len()].clone());
                }
            }
        } else {
            mem.timer = 9 + r % 8;
            mem.plan = Plan::Idle;
            match r {
                0..=299 => mem.press = Some(pick(&["x", "a", "x", "a", "y"], g.rand())),
                300..=479 => mem.press = Some(pick(&["y", "b", "z", "c"], g.rand())),
                480..=599 => {
                    mem.plan = Plan::Crouch;
                    mem.press = Some(pick(&["a", "b", "x", "y"], g.rand()));
                }
                600..=719 => {
                    let specials: Vec<String> = g.chars[c].def.cmds.cmds.iter().filter(|k| k.is_motion()).map(|k| k.name.clone()).collect();
                    if !specials.is_empty() {
                        mem.press = Some(specials[(g.rand() as usize) % specials.len()].clone());
                    }
                }
                720..=799 => (mem.plan, mem.timer) = (Plan::Jump, 4),
                800..=879 => (mem.plan, mem.timer) = (Plan::Back, 14),
                880..=939 => (mem.plan, mem.timer) = (Plan::Guard, 16),
                _ => {}
            }
        }
    }

    let holds: &[&str] = match mem.plan {
        Plan::Idle => &[],
        Plan::Forward => &["holdfwd"],
        Plan::Back | Plan::Guard => &["holdback"],
        Plan::CrouchGuard => &["holdback", "holddown"],
        Plan::Crouch => &["holddown"],
        Plan::Jump => &["holdup"],
        Plan::JumpForward => &["holdup", "holdfwd"],
    };
    let ch = &mut g.chars[c];
    let def = ch.def.clone();
    for h in holds {
        ch.cmd.force(&def.cmds, h);
    }
    // El botón sale un par de ticks después de decidir, con la postura ya puesta.
    if let Some(name) = &mem.press {
        if mem.timer <= 6 || mem.plan == Plan::Idle {
            ch.cmd.force(&def.cmds, name);
            mem.press = None;
        }
    }
    ch.ai_mem = mem;
}
