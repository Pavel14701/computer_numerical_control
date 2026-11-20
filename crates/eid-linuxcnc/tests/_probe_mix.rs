//! ВРЕМЕННЫЙ probe (удаляется).
use eid_core::adapter::Adapter;
use eid_linuxcnc::{LinuxCncAdapter, canonicalize::to_f64};
use eid_core::ast::{Op, StartPoint};

#[test]
fn probe_mix() {
    let src = concat!(
        "G21 G90 G94 G17 G90.1\n",
        "G0 X5.0000 Y0.0000 Z0.0000\n",
        "G3 X0.0000 Y5.0000 I0.0000 J0.0000 F100.0\n",
        "G91.1\n",
        "G2 X-5.0000 Y5.0000 I-5.0000 J0.0000 F120.0\n",
        "M2\n",
    );
    let p = LinuxCncAdapter::new().parse(src, "p.ngc");
    match p {
        Err(d) => for x in &d { println!("ERR: {}", x.message); },
        Ok(p) => {
            for (i, o) in p.ops.iter().enumerate() {
                match o {
                    Op::Arc { start: StartPoint::Known(sp), end, center, sweep_deg, dir, .. } => println!(
                        "OP{i} ARC s={:?} e={:?} c={:?} sw={:?} dir={dir:?}",
                        to_f64(sp), to_f64(end), to_f64(center), sweep_deg.value
                    ),
                    Op::Rapid { end, .. } => println!("OP{i} RAPID e={:?}", to_f64(end)),
                    Op::Linear { end, .. } => println!("OP{i} LINEAR e={:?}", to_f64(end)),
                    other => println!("OP{i} {:?}", std::mem::discriminant(other)),
                }
            }
        }
    }
    // тот же G2 в изоляции
    let src2 = "G21 G90 G94 G17 G91.1\nG0 X0.0000 Y5.0000 Z0.0000\nG2 X-5.0000 Y5.0000 I-5.0000 J0.0000 F120.0\nM2\n";
    match LinuxCncAdapter::new().parse(src2, "p2.ngc") {
        Err(d) => println!("ISO ERR: {}", d[0].message),
        Ok(p) => {
            if let Op::Arc { center, sweep_deg, dir, .. } = &p.ops[1] {
                println!("ISO ARC c={:?} sw={:?} dir={dir:?}", to_f64(center), sweep_deg.value);
            }
        }
    }
}
