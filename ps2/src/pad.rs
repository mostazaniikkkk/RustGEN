//! Mandos, con los módulos SIO2MAN y PADMAN de la BIOS.
//!
//! PADMAN escribe el estado de cada mando por DMA en un área de la RAM del
//! EE unas 60 veces por segundo; leerlo no cuesta ninguna llamada.

use crate::iop::{load_module, uncached, Aligned, Rpc};
use mugen::cmd::*;

const SID_PAD: u32 = 0x8000_010f;
const SID_PAD_EXT: u32 = 0x8000_011f;
const CMD_OPEN: u32 = 0x8000_0100;
const STATE_STABLE: u8 = 6;

pub struct Pad {
    /// Área de 256 bytes de cada puerto (PADMAN usa dos copias de 64).
    area: [*mut u8; 2],
    open: [bool; 2],
}

impl Pad {
    pub fn init() -> Pad {
        let mut pad = Pad { area: [std::ptr::null_mut(); 2], open: [false; 2] };
        // SIO2MAN puede estar ya cargado (lo usa también la tarjeta de memoria).
        let _ = std::os::ps2::load_iop_module("rom0:SIO2MAN");
        if !load_module("rom0:PADMAN") {
            return pad;
        }
        let Some(mut rpc) = Rpc::bind(SID_PAD, 200) else {
            println!("pad: PADMAN no responde");
            return pad;
        };
        let _ = Rpc::bind(SID_PAD_EXT, 200);
        for port in 0..2 {
            let area = uncached(Box::leak(Box::new(Aligned::<256>([0; 256]))).0.as_mut_ptr());
            unsafe {
                // Las dos copias empiezan "ocupadas" y sin datos.
                for copy in 0..2 {
                    let p = area.add(copy * 64);
                    std::ptr::write_bytes(p, 0, 64);
                    std::ptr::write_bytes(p.add(8), 0xff, 32);
                    p.add(4).write_volatile(5);
                    p.add(5).write_volatile(2);
                }
            }
            rpc.set(0, CMD_OPEN);
            rpc.set(1, port as u32);
            rpc.set(2, 0);
            rpc.set(3, 0);
            rpc.set(4, area as u32 & 0x1fff_ffff);
            rpc.call(1, 128, 128);
            pad.area[port] = area;
            pad.open[port] = rpc.get(3) != 0;
        }
        println!("pad: puertos abiertos {:?}", pad.open);
        pad
    }

    /// La copia más reciente del estado del puerto.
    fn data(&self, port: usize) -> Option<*const u8> {
        if !self.open[port] {
            return None;
        }
        let a = self.area[port] as *const u8;
        unsafe {
            let f0 = (a as *const u32).read_volatile();
            let f1 = (a.add(64) as *const u32).read_volatile();
            Some(if f0 < f1 { a.add(64) } else { a })
        }
    }

    /// Estado del mando: 0 = desconectado, 6 = listo.
    pub fn state(&self, port: usize) -> u8 {
        self.data(port).map_or(0, |d| unsafe { d.add(4).read_volatile() })
    }

    pub fn connected(&self, port: usize) -> bool {
        self.state(port) == STATE_STABLE
    }

    /// Botones pulsados, con los bits de PADMAN.
    pub fn buttons(&self, port: usize) -> u16 {
        let Some(d) = self.data(port) else { return 0 };
        unsafe {
            if d.add(4).read_volatile() != STATE_STABLE || d.add(8).read_volatile() != 0 {
                return 0;
            }
            // Bytes 2 y 3 de los datos: un bit por botón, 0 = pulsado.
            !(d.add(10).read_volatile() as u16 | (d.add(11).read_volatile() as u16) << 8)
        }
    }

    /// Teclas del motor: cruceta; cuadrado, triángulo y R1 = x, y, z;
    /// equis, círculo y R2 = a, b, c; START = s; SELECT = botón de sistema.
    pub fn keys(&self, port: usize) -> u16 {
        let b = self.buttons(port);
        let map = [
            (0x0010, IN_UP),
            (0x0040, IN_DOWN),
            (0x0080, IN_LEFT),
            (0x0020, IN_RIGHT),
            (0x8000, IN_X),
            (0x1000, IN_Y),
            (0x0800, IN_Z),
            (0x4000, IN_A),
            (0x2000, IN_B),
            (0x0200, IN_C),
            (0x0008, IN_S),
            (0x0001, IN_SELECT),
        ];
        map.iter().filter(|(bit, _)| b & bit != 0).fold(0, |k, (_, key)| k | key)
    }
}
