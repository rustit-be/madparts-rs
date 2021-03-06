// (c) 2016-2018 Joost Yervante Damad <joost@damad.be>

use chrono::{DateTime, Local};
use element::*;
use error::MpError;
use std::fs;
use std::io::Write;

#[derive(Default)]
pub struct Footprint {
    pub name: Option<Text>,
    pub reference: Option<Text>,
    pub desc: String,
    pub tags: String,
    pub pad: Vec<Pad>,
    pub smd: Vec<Smd>,
    pub lines: Vec<Line>,
    pub rects: Vec<Rect>,
}

fn to_footprint(elements: &Vec<Element>) -> Footprint {
    let mut f = Footprint::default();
    for e in elements {
        e.apply_footprint(&mut f);
    }
    f
}

pub fn save(elements: &Vec<Element>, f: &mut fs::File) -> Result<(), MpError> {
    let footprint = to_footprint(elements);
    // TODO
    let name = footprint
        .name
        .as_ref()
        .ok_or(MpError::Save("footprint is missing a name".into()))?;
    let reference = footprint
        .reference
        .as_ref()
        .ok_or(MpError::Save("footprint is missing a reference".into()))?;
    let local: DateTime<Local> = Local::now();
    let ts = local.timestamp();
    write!(f, "(module {} (layer F.Cu) (tedit {:X})\n", name.txt, ts)?;
    write!(f, "  (tags \"\")\n")?; // TODO tags
    write!(f, "  (attr smd)\n")?; // TODO pth

    write!(
        f,
        "  (fp_text reference REF** (at {} {}) (layer {})\n",
        reference.x, reference.y, reference.layer
    )?;
    write!(
        f,
        "    (effects (font (size {} {}) (thickness {})))",
        reference.dy, reference.dy, reference.thickness
    )?;
    write!(f, "  )\n")?;

    write!(
        f,
        "  (fp_text value {} (at {} {}) (layer {})\n",
        name.txt, name.x, name.y, name.layer
    )?;
    write!(
        f,
        "    (effects (font (size {} {}) (thickness {})))\n",
        name.dy, name.dy, name.thickness
    )?;
    write!(f, "  )\n")?;

    // TODO: maybe at some point allow overriding
    write!(f, "  (fp_text user %R (at 0 0) (layer F.Fab)\n")?;
    write!(f, "    (effects (font (size 0.8 0.8) (thickness 0.1)))\n")?;
    write!(f, "  )\n")?;

    for line in &footprint.lines {
        write!(
            f,
            "  (fp_line (start {} {}) (end {} {}) (layer {}) (width {}))\n",
            line.x1, line.y1, line.x2, line.y2, line.layer, line.w
        )?;
    }

    for pad in &footprint.smd {
        let layers = pad.layers
            .iter()
            .map(|l| format!("{}", l))
            .collect::<Vec<String>>()
            .join(" ");
        let shape:&'static str = pad.shape.clone().into();
        write!(
            f,
            "  (pad {} smd {} (at {} {}) (size {} {}) (layers {}))\n",
            pad.name, shape, pad.x, pad.y, pad.dx, pad.dy, layers
        )?;
    }

    for pad in &footprint.pad {
        let layers = pad.layers
            .iter()
            .map(|l| format!("{}", l))
            .collect::<Vec<String>>()
            .join(" ");
        let pad_type = if pad.plated {
            "thru_hole"
        } else {
            "np_thru_hole"
        };
        write!(
            f,
            "  (pad {} {} circle (at {} {}) (size {} {}) (drill {}) (layers {}))\n",
            pad.name, pad_type, pad.x, pad.y, pad.dx, pad.dy, pad.drill, layers
        )?;
    }

    for rect in &footprint.rects {
        write!(f, "  (fp_poly (pts ")?;
        write!(
            f,
            "(xy {} {})",
            rect.x - rect.dx / 2.0,
            rect.y - rect.dy / 2.0
        )?;
        write!(
            f,
            "(xy {} {})",
            rect.x + rect.dx / 2.0,
            rect.y - rect.dy / 2.0
        )?;
        write!(
            f,
            "(xy {} {})",
            rect.x + rect.dx / 2.0,
            rect.y + rect.dy / 2.0
        )?;
        write!(
            f,
            "(xy {} {})",
            rect.x - rect.dx / 2.0,
            rect.y + rect.dy / 2.0
        )?;
        write!(
            f,
            "(xy {} {})",
            rect.x - rect.dx / 2.0,
            rect.y - rect.dy / 2.0
        )?;
        write!(f, ") (layer {}) (width {}))\n", rect.layer, rect.w)?;
    }

    // TODO model...

    write!(f, ")\n")?;
    Ok(())
}
