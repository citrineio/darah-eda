use crate::model::{Id, Port, PortKind, Project};
use crate::power::Network;
use crate::units::Scale;
use serde::{Deserialize, Serialize};

pub const COMMON_MV: [u64; 18] = [900, 1000, 1100, 1200, 1500, 1800, 2500, 3300, 5000, 9000, 12000, 15000, 19000, 20000, 24000, 28000, 36000, 48000];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoltagePreset {
    pub nominal_mv: u64,
    pub tolerance_bp: u16,
}

impl VoltagePreset {
    pub const DEFAULT_TOLERANCE: u16 = 500;
    pub const MAX_TOLERANCE: u16 = 10_000;

    pub fn valid(self) -> bool {
        COMMON_MV.contains(&self.nominal_mv) && self.tolerance_bp <= Self::MAX_TOLERANCE
    }

    pub fn bounds(self) -> (u64, u64) {
        let center = self.nominal_mv * 1000;
        let offset = (center * u64::from(self.tolerance_bp) + 5_000) / 10_000;
        (center - offset, center + offset)
    }

    pub fn matching_default(bounds: (u64, u64)) -> Option<Self> {
        COMMON_MV.into_iter().map(|nominal_mv| Self { nominal_mv, tolerance_bp: Self::DEFAULT_TOLERANCE })
            .find(|preset| preset.bounds() == bounds)
    }
}

pub fn voltage_midpoint(min: u64, max: u64) -> Option<u64> {
    (min <= max && max > 0).then(|| (min + (max - min) / 2).max(1))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    Usb5, Supply12, Pd5, Pd9, Pd15, Pd20, Pd28, Pd36, Pd48, Avs28, Avs36, Avs48,
    LiIon, LiFePo4, NiMh, Alkaline, LeadAcid, CoinCell,
    BatteryCell(crate::electrical::Cell), DcSupply(u32), Bench(u32), Mains { volts: u16, hz: u16, phases: u8 },
    Transformer(u16), Telecom, Vehicle12, Vehicle24, AircraftDc, AircraftAc,
    Poe(u8), Solar, Generator,
}

impl Preset {
    pub const ALL: &[Self] = &[
        Self::Usb5, Self::Supply12, Self::Pd5, Self::Pd9, Self::Pd15, Self::Pd20, Self::Pd28,
        Self::Pd36, Self::Pd48, Self::Avs28, Self::Avs36, Self::Avs48,
        Self::LiIon, Self::LiFePo4, Self::NiMh, Self::Alkaline, Self::LeadAcid, Self::CoinCell,
        Self::DcSupply(3300), Self::DcSupply(5000), Self::DcSupply(6000), Self::DcSupply(7500),
        Self::DcSupply(9000), Self::DcSupply(13500), Self::DcSupply(15000), Self::DcSupply(18000),
        Self::DcSupply(19000), Self::DcSupply(19500), Self::DcSupply(20000), Self::DcSupply(24000),
        Self::DcSupply(27000), Self::DcSupply(28000), Self::DcSupply(30000), Self::DcSupply(36000),
        Self::DcSupply(42000), Self::DcSupply(48000), Self::DcSupply(54000), Self::DcSupply(60000),
        Self::Bench(30), Self::Bench(60),
        Self::Mains { volts: 100, hz: 50, phases: 1 }, Self::Mains { volts: 100, hz: 60, phases: 1 },
        Self::Mains { volts: 120, hz: 60, phases: 1 }, Self::Mains { volts: 127, hz: 60, phases: 1 },
        Self::Mains { volts: 220, hz: 50, phases: 1 }, Self::Mains { volts: 230, hz: 50, phases: 1 },
        Self::Mains { volts: 230, hz: 60, phases: 1 }, Self::Mains { volts: 240, hz: 50, phases: 1 },
        Self::Mains { volts: 277, hz: 60, phases: 1 }, Self::Mains { volts: 347, hz: 60, phases: 1 },
        Self::Mains { volts: 208, hz: 60, phases: 3 }, Self::Mains { volts: 400, hz: 50, phases: 3 },
        Self::Mains { volts: 400, hz: 60, phases: 3 }, Self::Mains { volts: 480, hz: 60, phases: 3 },
        Self::Mains { volts: 380, hz: 50, phases: 3 }, Self::Mains { volts: 415, hz: 50, phases: 3 },
        Self::Mains { volts: 440, hz: 60, phases: 3 }, Self::Mains { volts: 600, hz: 60, phases: 3 },
        Self::Mains { volts: 690, hz: 50, phases: 3 },
        Self::Transformer(6), Self::Transformer(9), Self::Transformer(12),
        Self::Transformer(15), Self::Transformer(18), Self::Transformer(24),
        Self::Telecom, Self::Vehicle12, Self::Vehicle24, Self::AircraftDc, Self::AircraftAc,
        Self::Poe(1), Self::Poe(2), Self::Poe(3), Self::Poe(4), Self::Solar, Self::Generator,
        Self::BatteryCell(crate::electrical::Cell::LiIon21700), Self::BatteryCell(crate::electrical::Cell::LiPoPouch),
        Self::BatteryCell(crate::electrical::Cell::NiMhAaa), Self::BatteryCell(crate::electrical::Cell::AlkalineAaa),
        Self::BatteryCell(crate::electrical::Cell::LithiumPrimaryAa), Self::BatteryCell(crate::electrical::Cell::SilverOxide),
        Self::BatteryCell(crate::electrical::Cell::ZincAir),
    ];

    pub fn label(self) -> String {
        match self {
            Self::Usb5 => "USB 5 V", Self::Supply12 => "Regulated 12 V supply",
            Self::Pd5 => "USB-C PD 5 V", Self::Pd9 => "USB-C PD 9 V", Self::Pd15 => "USB-C PD 15 V",
            Self::Pd20 => "USB-C PD 20 V", Self::Pd28 => "USB-C EPR 28 V",
            Self::Pd36 => "USB-C EPR 36 V", Self::Pd48 => "USB-C EPR 48 V",
            Self::Avs28 => "USB-C EPR AVS up to 28 V", Self::Avs36 => "USB-C EPR AVS up to 36 V",
            Self::Avs48 => "USB-C EPR AVS up to 48 V", Self::LiIon => "Li-ion / LiPo",
            Self::LiFePo4 => "LiFePO₄", Self::NiMh => "NiMH", Self::Alkaline => "Alkaline",
            Self::LeadAcid => "Lead-acid", Self::CoinCell => "Lithium coin cell",
            Self::BatteryCell(cell) => return cell.label().into(),
            Self::DcSupply(mv) => return format!("Regulated DC {} V", Scale::Milli.number(u128::from(mv))),
            Self::Bench(volts) => return format!("Bench supply 0–{volts} V DC"),
            Self::Mains { volts, hz, phases } => return format!("{volts} V AC RMS · {hz} Hz · {phases} phase"),
            Self::Transformer(volts) => return format!("Transformer secondary {volts} V AC RMS"),
            Self::Telecom => "Telecom −48 V DC", Self::Vehicle12 => "Vehicle 12 V DC",
            Self::Vehicle24 => "Vehicle 24 V DC", Self::AircraftDc => "Aircraft 28 V DC",
            Self::AircraftAc => "Aircraft 115 V AC · 400 Hz",
            Self::Poe(1) => "PoE PSE Type 1 · 44–57 V",
            Self::Poe(2) => "PoE PSE Type 2 · 50–57 V",
            Self::Poe(3) => "PoE PSE Type 3 · 50–57 V",
            Self::Poe(_) => "PoE PSE Type 4 · 52–57 V",
            Self::Solar => "Solar PV · enter the panel envelope",
            Self::Generator => "Generator · enter the AC envelope",
        }.into()
    }

    pub const CATEGORIES: [&'static str; 8] = [
        "Regulated DC supplies", "Bench and programmable supplies", "USB and USB-C PD",
        "AC mains", "Transformer secondaries", "Vehicle, aircraft and telecom",
        "PoE and energy sources", "Batteries",
    ];

    pub fn category(self) -> &'static str {
        match self {
            Self::Supply12 | Self::DcSupply(_) => Self::CATEGORIES[0], Self::Bench(_) => Self::CATEGORIES[1],
            Self::Mains { .. } | Self::Generator => Self::CATEGORIES[3],
            Self::Transformer(_) => Self::CATEGORIES[4],
            Self::Telecom | Self::Vehicle12 | Self::Vehicle24 | Self::AircraftDc | Self::AircraftAc => Self::CATEGORIES[5],
            Self::Poe(_) | Self::Solar => Self::CATEGORIES[6],
            _ if self.battery() => Self::CATEGORIES[7],
            _ => Self::CATEGORIES[2],
        }
    }

    pub fn power(self) -> crate::electrical::PowerSpec {
        use crate::electrical::{AcReference, Polarity, PowerKind, PowerSpec};
        let mut power = PowerSpec::default();
        if self == Self::Telecom { power.polarity = Polarity::Negative; }
        match self {
            Self::Mains { hz, phases, .. } => {
                power.kind = PowerKind::Ac;
                power.phases = phases;
                power.reference = if phases == 3 { AcReference::LineToLine } else { AcReference::LineToNeutral };
                power.frequency_min_millihz = Some(u64::from(hz) * 1000);
                power.frequency_max_millihz = power.frequency_min_millihz;
            }
            Self::Transformer(_) | Self::Generator | Self::AircraftAc => {
                power.kind = PowerKind::Ac;
                power.reference = AcReference::Secondary;
                let (min, max) = if self == Self::AircraftAc { (380_000, 420_000) } else { (45_000, 65_000) };
                power.frequency_min_millihz = Some(min);
                power.frequency_max_millihz = Some(max);
            }
            _ => {}
        }
        power
    }

    pub fn default_cell(self) -> Option<crate::electrical::Cell> {
        use crate::electrical::Cell;
        match self {
            Self::BatteryCell(cell) => Some(cell),
            Self::LiIon => Some(Cell::LiIon18650), Self::LiFePo4 => Some(Cell::LiFePo4Prismatic),
            Self::NiMh => Some(Cell::NiMhAa), Self::Alkaline => Some(Cell::AlkalineAa),
            Self::LeadAcid => Some(Cell::LeadAcid), Self::CoinCell => Some(Cell::Cr2032),
            _ => None,
        }
    }

    // The catalog selects chemistry; the separate cell picker selects format.
    pub fn catalog_entry(self) -> Self {
        use crate::electrical::{Cell, Chemistry};
        match self.default_cell().map(Cell::chemistry) {
            Some(Chemistry::LiIon) => Self::LiIon,
            Some(Chemistry::LiFePo4) => Self::LiFePo4,
            Some(Chemistry::NiMh) => Self::NiMh,
            Some(Chemistry::Alkaline) => Self::Alkaline,
            Some(Chemistry::LeadAcid) => Self::LeadAcid,
            Some(Chemistry::LithiumCoin) => Self::CoinCell,
            Some(Chemistry::LithiumPrimary) => Self::BatteryCell(Cell::LithiumPrimaryAa),
            Some(Chemistry::SilverOxide) => Self::BatteryCell(Cell::SilverOxide),
            Some(Chemistry::ZincAir) => Self::BatteryCell(Cell::ZincAir),
            None => self,
        }
    }

    pub fn catalog_label(self) -> String {
        self.default_cell().map_or_else(|| self.label(), |cell| cell.chemistry().label().into())
    }

    pub fn matches_power(self, power: crate::electrical::PowerSpec) -> bool {
        let expected = self.power();
        expected.kind == power.kind && expected.polarity == power.polarity
    }

    pub fn battery(self) -> bool {
        matches!(self, Self::LiIon | Self::LiFePo4 | Self::NiMh | Self::Alkaline | Self::LeadAcid | Self::CoinCell | Self::BatteryCell(_))
    }

    pub fn pd(self) -> bool {
        matches!(self, Self::Pd5 | Self::Pd9 | Self::Pd15 | Self::Pd20 | Self::Pd28 | Self::Pd36 | Self::Pd48
            | Self::Avs28 | Self::Avs36 | Self::Avs48)
    }

    // Starting assumptions in millivolts. The saved port bounds are independent of the preset.
    pub fn bounds(self, series: u16) -> (Option<u64>, u64, u64) {
        let fixed = |mv| (Some(mv), mv * 95 / 100, mv * 105 / 100);
        let (nominal, min, max) = match self {
            Self::Usb5 => (Some(5000), 4750, 5500), Self::Supply12 => fixed(12000),
            Self::Pd5 => fixed(5000), Self::Pd9 => fixed(9000), Self::Pd15 => fixed(15000),
            Self::Pd20 => fixed(20000), Self::Pd28 => fixed(28000), Self::Pd36 => fixed(36000),
            Self::Pd48 => fixed(48000),
            Self::Avs28 => (None, 15000, 28000), Self::Avs36 => (None, 15000, 36000),
            Self::Avs48 => (None, 15000, 48000),
            Self::LiIon => (Some(3600), 3000, 4200), Self::LiFePo4 => (Some(3200), 2500, 3650),
            Self::NiMh => (Some(1200), 1000, 1450), Self::Alkaline => (Some(1500), 900, 1600),
            Self::LeadAcid => (Some(2000), 1750, 2450), Self::CoinCell => (Some(3000), 2000, 3300),
            Self::BatteryCell(cell) => { let (min, max) = cell.bounds_mv(); (Some(cell.nominal_mv()), min, max) }
            Self::DcSupply(mv) => fixed(u64::from(mv)), Self::Bench(v) => (None, 0, u64::from(v) * 1000),
            Self::Mains { volts, .. } | Self::Transformer(volts) => {
                let mv = u64::from(volts) * 1000;
                (Some(mv), mv * 90 / 100, mv * 110 / 100)
            }
            Self::Telecom => (Some(48000), 40500, 57000),
            Self::Vehicle12 => (Some(12000), 9000, 16000), Self::Vehicle24 => (Some(24000), 18000, 32000),
            Self::AircraftDc => (Some(28000), 22000, 29000), Self::AircraftAc => (Some(115000), 103500, 126500),
            Self::Poe(1) => (Some(48000), 44000, 57000), Self::Poe(2 | 3) => (Some(54000), 50000, 57000),
            Self::Poe(_) => (Some(54000), 52000, 57000),
            Self::Solar | Self::Generator => (None, 0, 0),
        };
        let scale = if self.battery() { u64::from(series) * 1000 } else { 1000 };
        (nominal.map(|value| value * scale), min * scale, max * scale)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub preset: Preset,
    pub series: u16,
    #[serde(default)]
    pub contract_confirmed: bool,
}

pub fn needs_contract(port: &Port) -> bool {
    port.source.is_some_and(|source| source.preset.pd() && !source.contract_confirmed)
}

pub fn fits(rail: &Port, input: &Port) -> Option<bool> {
    if rail.power.kind != input.power.kind
        || rail.power.polarity != input.power.polarity
        || !rail.rail_domain.trim().eq_ignore_ascii_case(input.rail_domain.trim()) {
        return Some(false);
    }
    if rail.power.kind == crate::electrical::PowerKind::Ac {
        if rail.power.phases != input.power.phases || rail.power.reference != input.power.reference {
            return Some(false);
        }
        let ((min_hz, max_hz), (accept_min, accept_max)) = (rail.power.frequency()?, input.power.frequency()?);
        if min_hz < accept_min || max_hz > accept_max { return Some(false); }
    }
    let ((rail_min, rail_max), (input_min, input_max)) = (rail.range()?, input.range()?);
    Some(input_min <= rail_min && rail_max <= input_max)
}

pub struct SupplyNeed<'a> {
    pub input: &'a Port,
    pub compatible: Vec<&'a Port>,
    pub pending: Vec<&'a Port>,
    pub unspecified: Vec<&'a Port>,
}

impl SupplyNeed<'_> {
    pub fn missing(&self) -> bool {
        self.input.range().is_some() && self.input.power.complete() && self.compatible.is_empty() && self.pending.is_empty()
    }
}

pub fn unmet<'a>(project: &'a Project, network: &Network) -> Vec<SupplyNeed<'a>> {
    let bound: std::collections::HashSet<_> = project.bindings.iter().map(|binding| binding.target).collect();
    let rails: Vec<_> = project.ports.iter().filter(|port| port.kind == PortKind::Rail).collect();
    project.ports.iter().filter(|port| port.kind == PortKind::Load && port.required && !bound.contains(&port.id))
        .map(|input| {
            let excluded = network.downstream(input.id);
            let mut need = SupplyNeed { input, compatible: Vec::new(), pending: Vec::new(), unspecified: Vec::new() };
            for &rail in &rails {
                if excluded.contains(&rail.id) { continue; }
                match fits(rail, input) {
                    Some(true) if network.awaiting_contract(rail.id) => need.pending.push(rail),
                    Some(true) => need.compatible.push(rail),
                    None if input.range().is_some() => need.unspecified.push(rail),
                    _ => {}
                }
            }
            need
        }).collect()
}

pub fn label_voltage(uv: u64) -> String { Scale::Base.display(uv.into(), "V") }

pub fn proposal(needs: &[SupplyNeed<'_>], input: &Port) -> (Vec<Id>, (u64, u64)) {
    let Some((mut min, mut max)) = input.range() else { return (Vec::new(), (0, 0)); };
    let mut ports = vec![input.id];
    // Group matching effective nominal voltages, power characteristics, and domains.
    if input.nominal_voltage().is_some() {
        for other in needs.iter().filter(|other| other.missing()).map(|other| other.input) {
            if other.id == input.id || other.nominal_voltage() != input.nominal_voltage() || other.power != input.power || !other.rail_domain.trim().eq_ignore_ascii_case(input.rail_domain.trim()) { continue; }
            if let Some((lo, hi)) = other.range() {
                min = min.max(lo);
                max = max.min(hi);
                ports.push(other.id);
            }
        }
    }
    (ports, (min, max))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Result, Role};

    #[test]
    fn battery_catalog_groups_formats_by_chemistry() {
        use crate::electrical::{Cell, Chemistry};
        let families: Vec<_> = Preset::ALL.iter().copied()
            .filter(|preset| preset.battery() && preset.catalog_entry() == *preset).collect();
        assert_eq!(families.len(), 9);
        let lithium: Vec<_> = Cell::ALL.into_iter().filter(|cell| cell.chemistry() == Chemistry::LiIon).collect();
        assert_eq!(lithium, vec![Cell::LiIon18650, Cell::LiIon21700, Cell::LiPoPouch]);
        assert_eq!(Preset::BatteryCell(Cell::NiMhAaa).catalog_entry(), Preset::NiMh);
        assert_eq!(Preset::BatteryCell(Cell::LiIon21700).catalog_entry(), Preset::LiIon);
        assert!(!lithium.contains(&Cell::LiFePo4Prismatic));
    }

    #[test]
    fn fixed_sources_cannot_change_ac_dc_or_polarity() {
        use crate::electrical::{Polarity, PowerKind};
        for &preset in Preset::ALL {
            let mut power = preset.power();
            assert!(preset.matches_power(power));
            power.kind = if power.kind == PowerKind::Dc { PowerKind::Ac } else { PowerKind::Dc };
            assert!(!preset.matches_power(power));
            power = preset.power();
            power.polarity = if power.polarity == Polarity::Positive { Polarity::Negative } else { Polarity::Positive };
            assert!(!preset.matches_power(power));
        }
    }

    #[test]
    fn presets_scale_batteries_and_leave_avs_nominal_undecided() {
        assert_eq!(Preset::LiIon.bounds(2), (Some(7_200_000), 6_000_000, 8_400_000));
        assert_eq!(Preset::Pd48.bounds(1), (Some(48_000_000), 45_600_000, 50_400_000));
        assert_eq!(Preset::Avs48.bounds(1), (None, 15_000_000, 48_000_000));
    }

    #[test]
    fn ac_and_dc_and_incomplete_frequency_are_distinct() {
        use crate::electrical::{PowerKind, PowerSpec};
        let mut output = Port::new(1, PortKind::Rail);
        let mut input = Port::new(2, PortKind::Load);
        output.min_uv = Some(220_000_000);
        output.max_uv = Some(240_000_000);
        input.min_uv = Some(200_000_000);
        input.max_uv = Some(250_000_000);
        output.power = PowerSpec { kind: PowerKind::Ac, frequency_min_millihz: Some(50_000),
            frequency_max_millihz: Some(50_000), ..PowerSpec::default() };
        assert_eq!(fits(&output, &input), Some(false));
        input.power.kind = PowerKind::Ac;
        assert_eq!(fits(&output, &input), None);
        input.power.frequency_min_millihz = Some(45_000);
        input.power.frequency_max_millihz = Some(65_000);
        assert_eq!(fits(&output, &input), Some(true));
    }

    #[test]
    fn missing_rails_respect_range_domains_and_loops() -> Result<()> {
        let mut project = crate::power::example(false)?;
        let first_output = project.conversions[0].output;
        let first_input = project.conversions[0].input;
        let second_output = project.conversions[1].output;
        let second_input = project.conversions[1].input;
        let incoming = project.bindings.iter().find(|wire| wire.target == second_input).unwrap().id;
        project.unbind(incoming);
        let network = Network::new(&project, None);
        let needs = unmet(&project, &network);
        let need = needs.iter().find(|need| need.input.id == second_input).unwrap();
        assert!(need.compatible.iter().any(|rail| rail.id == first_output));
        assert!(!need.compatible.iter().any(|rail| rail.id == second_output));
        assert!(!network.downstream(first_input).is_empty());
        let mut input = project.port(second_input).unwrap().clone();
        input.rail_domain = "isolated".into();
        project.set_port(input)?;
        let needs = unmet(&project, &Network::new(&project, None));
        assert!(needs.iter().find(|need| need.input.id == second_input).unwrap().missing());
        Ok(())
    }

    #[test]
    fn unconfirmed_pd_propagates_through_converters() -> Result<()> {
        let mut project = crate::power::example(false)?;
        let input = project.conversions[0].input;
        let source = project.bindings.iter().find(|wire| wire.target == input).unwrap().source;
        let output = project.conversions[1].output;
        let mut port = project.port(source).unwrap().clone();
        port.source = Some(Source { preset: Preset::Pd15, series: 1, contract_confirmed: false });
        project.set_port(port.clone())?;
        assert!(Network::new(&project, None).awaiting_contract(output));
        port.source.as_mut().unwrap().contract_confirmed = true;
        project.set_port(port)?;
        assert!(!Network::new(&project, None).awaiting_contract(output));
        Ok(())
    }

    #[test]
    fn shared_proposals_keep_different_domains_separate() -> Result<()> {
        let mut project = Project::new();
        for domain in ["digital", "digital", "analog"] {
            let block = project.add_block(Role::Custom, domain.into(), false)?;
            let mut input = Port::new(block, PortKind::Load);
            input.nominal_uv = Some(3_300_000);
            input.min_uv = Some(3_100_000);
            input.max_uv = Some(3_500_000);
            input.rail_domain = domain.into();
            project.set_port(input)?;
        }
        let needs = unmet(&project, &Network::new(&project, None));
        assert_eq!(proposal(&needs, needs[0].input).0.len(), 2);
        assert_eq!(proposal(&needs, needs[2].input).0.len(), 1);
        Ok(())
    }
}
