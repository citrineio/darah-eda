use crate::model::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerKind { #[default] Dc, Ac }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Polarity { #[default] Positive, Negative }

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcReference { #[default] LineToNeutral, LineToLine, Secondary }

impl AcReference {
    pub fn label(self) -> &'static str {
        match self { Self::LineToNeutral => "Line to neutral", Self::LineToLine => "Line to line", Self::Secondary => "Transformer secondary" }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PowerSpec {
    pub kind: PowerKind,
    pub polarity: Polarity,
    pub phases: u8,
    pub reference: AcReference,
    pub frequency_min_millihz: Option<u64>,
    pub frequency_max_millihz: Option<u64>,
}

impl Default for PowerSpec {
    fn default() -> Self {
        Self { kind: PowerKind::Dc, polarity: Polarity::Positive, phases: 1,
            reference: AcReference::LineToNeutral, frequency_min_millihz: None, frequency_max_millihz: None }
    }
}

impl PowerSpec {
    pub fn is_default(&self) -> bool { *self == Self::default() }
    pub fn label(self) -> String {
        match self.kind {
            PowerKind::Dc => match self.polarity {
                Polarity::Positive => "Positive DC".into(), Polarity::Negative => "Negative DC".into(),
            },
            PowerKind::Ac => format!("AC RMS · {} phase · {}", self.phases, self.reference.label()),
        }
    }
    pub fn positive_dc(self) -> bool { self.kind == PowerKind::Dc && self.polarity == Polarity::Positive }
    pub fn complete(self) -> bool { self.kind == PowerKind::Dc || self.frequency().is_some() }
    pub fn frequency(self) -> Option<(u64, u64)> {
        Some((self.frequency_min_millihz?, self.frequency_max_millihz?))
    }
    pub fn validate(self) -> Result<()> {
        if !matches!(self.phases, 1 | 3) || self.frequency().is_some_and(|(min, max)| min > max)
            || self.frequency_min_millihz == Some(0) || self.frequency_max_millihz == Some(0)
            || self.kind == PowerKind::Dc && self.phases != 1
        {
            return Err("Use one or three AC phases and a positive, ordered frequency interval.".into());
        }
        if self.kind == PowerKind::Ac && self.polarity != Polarity::Positive {
            return Err("AC voltage uses RMS magnitude and has no DC polarity.".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapacityUnit { #[default] Mah, Mwh }

impl CapacityUnit {
    pub fn label(self) -> &'static str { match self { Self::Mah => "mAh", Self::Mwh => "mWh" } }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capacity {
    pub unit: CapacityUnit,
    // One thousandth of the displayed mAh or mWh.
    pub milli_value: u64,
}

impl Capacity {
    pub fn convert(self, unit: CapacityUnit, nominal_uv: u64) -> Result<Self> {
        if self.unit == unit { return Ok(self); }
        if nominal_uv == 0 { return Err("Declare a positive pack nominal voltage before converting capacity.".into()); }
        let value = match unit {
            CapacityUnit::Mwh => u128::from(self.milli_value) * u128::from(nominal_uv) / 1_000_000,
            CapacityUnit::Mah => u128::from(self.milli_value) * 1_000_000 / u128::from(nominal_uv),
        };
        Ok(Self { unit, milli_value: u64::try_from(value).ok().filter(|&value| value <= i64::MAX as u64).ok_or("Capacity is too large.")? })
    }
    pub fn energy_uwh(self, nominal_uv: u64) -> Result<u64> {
        Ok(self.convert(CapacityUnit::Mwh, nominal_uv)?.milli_value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cell {
    LiIon18650, LiIon21700, LiPoPouch, LiFePo4Prismatic, NiMhAa, NiMhAaa, AlkalineAa,
    AlkalineAaa, LeadAcid, Cr2032, LithiumPrimaryAa, SilverOxide, ZincAir,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chemistry { LiIon, LiFePo4, NiMh, Alkaline, LeadAcid, LithiumCoin, LithiumPrimary, SilverOxide, ZincAir }

impl Chemistry {
    pub fn label(self) -> &'static str {
        match self {
            Self::LiIon => "Li-ion / LiPo", Self::LiFePo4 => "LiFePO₄", Self::NiMh => "NiMH",
            Self::Alkaline => "Alkaline", Self::LeadAcid => "Lead-acid", Self::LithiumCoin => "Lithium coin cell",
            Self::LithiumPrimary => "Primary lithium", Self::SilverOxide => "Silver oxide", Self::ZincAir => "Zinc-air",
        }
    }
}

impl Cell {
    pub const ALL: [Self; 13] = [
        Self::LiIon18650, Self::LiIon21700, Self::LiPoPouch, Self::LiFePo4Prismatic,
        Self::NiMhAa, Self::NiMhAaa, Self::AlkalineAa, Self::AlkalineAaa, Self::LeadAcid,
        Self::Cr2032, Self::LithiumPrimaryAa, Self::SilverOxide, Self::ZincAir,
    ];
    pub fn chemistry(self) -> Chemistry {
        match self {
            Self::LiIon18650 | Self::LiIon21700 | Self::LiPoPouch => Chemistry::LiIon,
            Self::LiFePo4Prismatic => Chemistry::LiFePo4,
            Self::NiMhAa | Self::NiMhAaa => Chemistry::NiMh,
            Self::AlkalineAa | Self::AlkalineAaa => Chemistry::Alkaline,
            Self::LeadAcid => Chemistry::LeadAcid, Self::Cr2032 => Chemistry::LithiumCoin,
            Self::LithiumPrimaryAa => Chemistry::LithiumPrimary,
            Self::SilverOxide => Chemistry::SilverOxide, Self::ZincAir => Chemistry::ZincAir,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::LiIon18650 => "Li-ion 18650 · reference 2800 mAh",
            Self::LiIon21700 => "Li-ion 21700 · reference 4500 mAh",
            Self::LiPoPouch => "LiPo pouch · illustrative 1000 mAh",
            Self::LiFePo4Prismatic => "LiFePO₄ prismatic · illustrative 100 Ah",
            Self::NiMhAa => "NiMH AA · reference 1980 mAh",
            Self::NiMhAaa => "NiMH AAA · illustrative 750 mAh",
            Self::AlkalineAa => "Alkaline AA · illustrative 2500 mAh",
            Self::AlkalineAaa => "Alkaline AAA · illustrative 1000 mAh",
            Self::LeadAcid => "Lead-acid cell · 7 Ah at 20-hour rate",
            Self::Cr2032 => "CR2032 · reference 225 mAh",
            Self::LithiumPrimaryAa => "Primary lithium AA · illustrative 3000 mAh",
            Self::SilverOxide => "Silver-oxide button · illustrative 150 mAh",
            Self::ZincAir => "Zinc-air button · illustrative 180 mAh",
        }
    }
    pub fn nominal_mv(self) -> u64 {
        match self {
            Self::LiIon18650 | Self::LiIon21700 => 3600, Self::LiPoPouch => 3700,
            Self::LiFePo4Prismatic => 3200, Self::NiMhAa | Self::NiMhAaa => 1200,
            Self::LeadAcid => 2000, Self::Cr2032 => 3000, Self::SilverOxide => 1550,
            Self::ZincAir => 1450, _ => 1500,
        }
    }
    pub fn bounds_mv(self) -> (u64, u64) {
        match self {
            Self::LiIon18650 | Self::LiIon21700 | Self::LiPoPouch => (3000, 4200),
            Self::LiFePo4Prismatic => (2500, 3650), Self::NiMhAa | Self::NiMhAaa => (1000, 1450),
            Self::LeadAcid => (1750, 2450), Self::Cr2032 => (2000, 3300),
            Self::LithiumPrimaryAa => (900, 1800), Self::SilverOxide => (1200, 1650),
            Self::ZincAir => (1000, 1500), _ => (900, 1600),
        }
    }
    pub fn capacity_mah(self) -> u64 {
        match self {
            Self::LiIon18650 => 2800, Self::LiIon21700 => 4500, Self::LiPoPouch => 1000,
            Self::LiFePo4Prismatic => 100_000, Self::NiMhAa => 1980, Self::NiMhAaa => 750,
            Self::AlkalineAa => 2500, Self::AlkalineAaa => 1000, Self::LeadAcid => 7000,
            Self::Cr2032 => 225, Self::LithiumPrimaryAa => 3000, Self::SilverOxide => 150,
            Self::ZincAir => 180,
        }
    }
    pub fn default_pack(self, parallel: u16) -> Battery {
        Battery { cell: self, parallel, capacity: Some(Capacity { unit: CapacityUnit::Mah,
            milli_value: self.capacity_mah() * 1000 * u64::from(parallel) }) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Battery {
    pub cell: Cell,
    pub parallel: u16,
    pub capacity: Option<Capacity>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_converts_using_pack_nominal_voltage() -> Result<()> {
        let pack = Cell::LiIon18650.default_pack(2);
        let charge = pack.capacity.unwrap();
        let energy = charge.convert(CapacityUnit::Mwh, 7_200_000)?;
        assert_eq!(energy.milli_value, 40_320_000);
        assert_eq!(energy.convert(CapacityUnit::Mah, 7_200_000)?, charge);
        assert!(charge.convert(CapacityUnit::Mwh, 0).is_err());
        Ok(())
    }
}
