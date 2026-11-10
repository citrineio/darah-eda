use crate::model::Result;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scale { Base, Milli, Micro }

impl Scale {
    pub const ALL: [Self; 3] = [Self::Base, Self::Milli, Self::Micro];

    fn decimals(self) -> u32 {
        match self { Self::Base => 6, Self::Milli => 3, Self::Micro => 0 }
    }

    pub fn label(self, symbol: &str) -> String {
        let prefix = match self { Self::Base => "", Self::Milli => "m", Self::Micro => "µ" };
        format!("{prefix}{symbol}")
    }

    pub fn for_value(micro: u128) -> Self {
        match micro {
            0 => Self::Milli,
            1..=999 => Self::Micro,
            1000..=999_999 => Self::Milli,
            _ => Self::Base,
        }
    }

    pub fn number(self, micro: u128) -> String {
        let factor = 10_u128.pow(self.decimals());
        let whole = micro / factor;
        let remainder = micro % factor;
        if remainder == 0 { return whole.to_string(); }
        let fraction = format!("{remainder:0width$}", width = self.decimals() as usize);
        format!("{whole}.{}", fraction.trim_end_matches('0'))
    }

    pub fn parse(self, text: &str) -> Result<Option<u64>> {
        let text = text.trim();
        if text.is_empty() { return Ok(None); }
        let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
        if (whole.is_empty() && fraction.is_empty())
            || !whole.bytes().chain(fraction.bytes()).all(|byte| byte.is_ascii_digit())
        {
            return Err("Enter a nonnegative decimal number, or leave the field blank.".into());
        }
        let fraction = fraction.trim_end_matches('0');
        let decimals = self.decimals();
        if fraction.len() > decimals as usize {
            return Err("The smallest supported increment is one micro-unit.".into());
        }
        let out_of_range = "Value is too large.";
        let integer: u64 = if whole.is_empty() { 0 } else { whole.parse().map_err(|_| out_of_range)? };
        let fractional: u64 = if fraction.is_empty() { 0 } else { fraction.parse().map_err(|_| out_of_range)? };
        let value = integer.checked_mul(10_u64.pow(decimals))
            .and_then(|value| value.checked_add(fractional * 10_u64.pow(decimals - fraction.len() as u32)))
            .filter(|&value| value <= i64::MAX as u64)
            .ok_or(out_of_range)?;
        Ok(Some(value))
    }

    pub fn display(self, micro: u128, symbol: &str) -> String {
        format!("{} {}", self.number(micro), self.label(symbol))
    }
}
