use crate::{model::{Port, Result}, review::Status};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default] Digital, AnalogVoltage, AnalogCurrent, Clock, Pwm, DifferentialDigital, DifferentialAnalog, Contact, Rf,
}

impl Kind {
    pub const ALL: [Self; 9] = [Self::Digital, Self::AnalogVoltage, Self::AnalogCurrent, Self::Clock,
        Self::Pwm, Self::DifferentialDigital, Self::DifferentialAnalog, Self::Contact, Self::Rf];
    pub fn label(self) -> &'static str {
        match self {
            Self::Digital => "Digital", Self::AnalogVoltage => "Analog voltage", Self::AnalogCurrent => "Analog current",
            Self::Clock => "Clock", Self::Pwm => "PWM", Self::DifferentialDigital => "Differential digital",
            Self::DifferentialAnalog => "Differential analog", Self::Contact => "Contact / switch", Self::Rf => "RF / transmission line",
        }
    }
    pub fn logic(self) -> bool { matches!(self, Self::Digital | Self::Clock | Self::Pwm) }
    pub fn differential(self) -> bool { matches!(self, Self::DifferentialDigital | Self::DifferentialAnalog) }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Drive { #[default] Unspecified, PushPull, OpenDrain, OpenSource, TriState, Passive }

impl Drive {
    pub const ALL: [Self; 6] = [Self::Unspecified, Self::PushPull, Self::OpenDrain, Self::OpenSource, Self::TriState, Self::Passive];
    pub fn label(self) -> &'static str {
        match self {
            Self::Unspecified => "Unspecified", Self::PushPull => "Push-pull", Self::OpenDrain => "Open drain / collector",
            Self::OpenSource => "Open source", Self::TriState => "Three-state", Self::Passive => "Passive",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Parameter {
    VoltageMin, VoltageMax, CurrentMin, CurrentMax, HighMin, LowMax, AbsMin, AbsMax,
    CommonMin, CommonMax, SourceCurrent, SinkCurrent, Leakage, Impedance, ComplianceMin, ComplianceMax,
    FrequencyMin, FrequencyMax, Bandwidth, Rise, Fall, Delay, Jitter, MinPulse, Setup, Hold, Skew,
    Capacitance, Termination, PullResistance, PullVoltageMin, PullVoltageMax,
    Accuracy, Noise, SampleRate, Resolution, DutyMin, DutyMax, DataRate, CableLength, UnitLoads,
}

impl Parameter {
    pub fn label(self, output: bool) -> &'static str {
        use Parameter::*;
        match self {
            VoltageMin => "Minimum signal voltage", VoltageMax => "Maximum signal voltage",
            CurrentMin => "Minimum signal current", CurrentMax => "Maximum signal current",
            HighMin if output => "Guaranteed VOH minimum", HighMin => "Required VIH minimum",
            LowMax if output => "Guaranteed VOL maximum", LowMax => "Allowed VIL maximum",
            AbsMin => "Absolute minimum voltage", AbsMax => "Absolute maximum voltage",
            CommonMin => "Minimum common-mode voltage", CommonMax => "Maximum common-mode voltage",
            SourceCurrent if output => "Guaranteed source current", SourceCurrent => "Required high-state current",
            SinkCurrent if output => "Guaranteed sink current", SinkCurrent => "Required low-state current",
            Leakage => "Maximum leakage current",
            Impedance if output => "Output impedance", Impedance => "Input impedance",
            ComplianceMin => "Minimum compliance voltage", ComplianceMax => "Maximum compliance voltage",
            FrequencyMin => "Minimum signal frequency", FrequencyMax => "Maximum signal frequency",
            Bandwidth => "Analog bandwidth", Rise => "Rise time", Fall => "Fall time", Delay => "Propagation delay",
            Jitter => "Jitter", MinPulse => "Minimum pulse width", Setup => "Setup time", Hold => "Hold time",
            Skew => "Skew", Capacitance if output => "Maximum capacitive load", Capacitance => "Input capacitance",
            Termination => "Termination resistance", PullResistance => "Pull resistance",
            PullVoltageMin => "Minimum pull voltage", PullVoltageMax => "Maximum pull voltage",
            Accuracy => "Maximum error", Noise => "Noise RMS", SampleRate => "Sampling / update rate",
            Resolution => "Resolution", DutyMin => "Minimum duty cycle", DutyMax => "Maximum duty cycle",
            DataRate => "Data rate", CableLength => "Cable length", UnitLoads => "Bus unit loads",
        }
    }
    pub fn unit(self) -> (&'static str, u32) {
        use Parameter::*;
        match self {
            VoltageMin | VoltageMax | HighMin | LowMax | AbsMin | AbsMax | CommonMin | CommonMax
                | ComplianceMin | ComplianceMax | PullVoltageMin | PullVoltageMax => ("V", 6),
            CurrentMin | CurrentMax | SourceCurrent | SinkCurrent | Leakage => ("mA", 3),
            Impedance | Termination | PullResistance => ("Ω", 0),
            FrequencyMin | FrequencyMax | Bandwidth => ("Hz", 0),
            Rise | Fall | Delay | MinPulse | Setup | Hold => ("ns", 0), Jitter | Skew => ("ps", 0),
            Capacitance => ("pF", 0), Accuracy => ("ppm", 0), Noise => ("nV RMS", 0),
            SampleRate => ("samples/s", 0), Resolution => ("bits", 0),
            DutyMin | DutyMax => ("%", 4), DataRate => ("bit/s", 0), CableLength => ("m", 3), UnitLoads => ("UL", 3),
        }
    }
    fn signed(self) -> bool {
        use Parameter::*;
        matches!(self, VoltageMin | VoltageMax | CurrentMin | CurrentMax | HighMin | LowMax
            | AbsMin | AbsMax | CommonMin | CommonMax | ComplianceMin | ComplianceMax | PullVoltageMin | PullVoltageMax)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Spec {
    pub kind: Kind,
    pub drive: Drive,
    pub reference: String,
    pub conditions: String,
    pub power_off_behavior: String,
    pub values: BTreeMap<Parameter, i64>,
    pub additional: Vec<Additional>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Additional {
    pub name: String,
    pub value: String,
    pub unit: String,
    pub conditions: String,
}

pub fn parse(text: &str, decimals: u32) -> Result<Option<i64>> {
    let text = text.trim();
    if text.is_empty() { return Ok(None); }
    let negative = text.starts_with('-');
    let text = text.strip_prefix('-').or_else(|| text.strip_prefix('+')).unwrap_or(text);
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty() && fraction.is_empty() {
        return Err("Enter a decimal value using the displayed unit.".into());
    }
    let fraction = fraction.trim_end_matches('0');
    if fraction.len() > decimals as usize
        || !whole.bytes().chain(fraction.bytes()).all(|digit| digit.is_ascii_digit())
    { return Err("Enter a decimal value using the displayed unit.".into()); }
    let whole: i128 = if whole.is_empty() { 0 } else { whole.parse().map_err(|_| "Value is too large.")? };
    let part: i128 = if fraction.is_empty() { 0 } else { fraction.parse().map_err(|_| "Value is too large.")? };
    let magnitude = whole.checked_mul(10_i128.pow(decimals))
        .and_then(|value| value.checked_add(part * 10_i128.pow(decimals - fraction.len() as u32)))
        .ok_or("Value is too large.")?;
    i64::try_from(if negative { -magnitude } else { magnitude }).map(Some).map_err(|_| "Value is too large.".into())
}

pub fn number(value: i64, decimals: u32) -> String {
    let value = i128::from(value);
    let sign = if value < 0 { "-" } else { "" };
    let value = value.abs();
    let scale = 10_i128.pow(decimals);
    let remainder = value % scale;
    if remainder == 0 { return format!("{sign}{}", value / scale); }
    let fraction = format!("{remainder:0width$}", width = decimals as usize);
    format!("{sign}{}.{}", value / scale, fraction.trim_end_matches('0'))
}

pub fn validate(spec: &Spec) -> Result<()> {
    for (&key, &value) in &spec.values {
        if (!key.signed() && value < 0) || matches!(key, Parameter::DutyMin | Parameter::DutyMax) && value > 1_000_000 {
            return Err(format!("{} is outside its allowed range.", key.label(false)));
        }
    }
    use Parameter::*;
    for (min, max) in [(VoltageMin, VoltageMax), (CurrentMin, CurrentMax), (AbsMin, AbsMax), (CommonMin, CommonMax),
        (ComplianceMin, ComplianceMax), (FrequencyMin, FrequencyMax), (PullVoltageMin, PullVoltageMax), (DutyMin, DutyMax)]
    {
        if spec.values.get(&min).zip(spec.values.get(&max)).is_some_and(|(min, max)| min > max) {
            return Err(format!("{} exceeds its maximum.", min.label(false)));
        }
    }
    if spec.additional.iter().any(|field| field.name.trim().is_empty()) {
        return Err("Give each additional signal parameter a name.".into());
    }
    Ok(())
}

pub fn evaluate(source: &Port, target: &Port) -> (Status, String, Vec<String>) {
    let mut checks = Vec::new();
    let (left, right) = (source.signal.trim(), target.signal.trim());
    checks.push(if left.is_empty() || right.is_empty() {
        (Status::Open, "Declare the interface contract on both ports.".into())
    } else if left.eq_ignore_ascii_case(right) {
        (Status::Pass, format!("Interface contracts match ({left})."))
    } else {
        (Status::Fail, format!("Interface contracts differ ({left} vs {right})."))
    });
    match (&source.signal_spec, &target.signal_spec) {
        (None, None) => {}
        (Some(output), Some(input)) => {
            if output.kind != input.kind { checks.push((Status::Fail, "Signal types differ.".into())); }
            else {
                use Parameter::*;
                let compare = |a: Parameter, b: Parameter, greater: bool, name: &str| {
                    match (output.values.get(&a), input.values.get(&b)) {
                        (Some(a), Some(b)) => {
                            let compatible = if greater { a >= b } else { a <= b };
                            (if compatible { Status::Pass } else { Status::Fail }, name.into())
                        }
                        _ => (Status::Open, format!("{name} · values missing")),
                    }
                };
                if output.reference.trim().is_empty() || input.reference.trim().is_empty() {
                    checks.push((Status::Open, "Declare the signal reference on both ports.".into()));
                } else if !output.reference.trim().eq_ignore_ascii_case(input.reference.trim()) {
                    checks.push((Status::Open, "Signal references differ. Review the reference or isolation boundary.".into()));
                }
                if output.kind.logic() {
                    if matches!(output.drive, Drive::PushPull | Drive::TriState) {
                        checks.push(compare(HighMin, HighMin, true, "High-level margin"));
                        checks.push(compare(LowMax, LowMax, false, "Low-level margin"));
                        checks.push(compare(SourceCurrent, SourceCurrent, true, "Source-current capacity"));
                        checks.push(compare(SinkCurrent, SinkCurrent, true, "Sink-current capacity"));
                    } else {
                        checks.push((Status::Open, "Review output stage, pull network, and loaded logic levels.".into()));
                    }
                }
                if matches!(output.kind, Kind::AnalogVoltage | Kind::DifferentialAnalog) {
                    checks.push(compare(VoltageMin, VoltageMin, true, "Minimum analog voltage"));
                    checks.push(compare(VoltageMax, VoltageMax, false, "Maximum analog voltage"));
                }
                if output.kind == Kind::AnalogCurrent {
                    checks.push(compare(CurrentMin, CurrentMin, true, "Minimum analog current"));
                    checks.push(compare(CurrentMax, CurrentMax, false, "Maximum analog current"));
                    checks.push((Status::Manual, "Review compliance voltage and loop burden at the declared current.".into()));
                }
                if output.kind.differential() {
                    checks.push(compare(CommonMin, CommonMin, true, "Minimum common-mode voltage"));
                    checks.push(compare(CommonMax, CommonMax, false, "Maximum common-mode voltage"));
                    if output.kind == Kind::DifferentialDigital {
                        checks.push((Status::Manual, "Review differential thresholds, termination, loading, and timing.".into()));
                    }
                }
                if matches!(output.kind, Kind::Clock | Kind::Pwm) {
                    checks.push(compare(FrequencyMin, FrequencyMin, true, "Minimum clock / PWM frequency"));
                    checks.push(compare(FrequencyMax, FrequencyMax, false, "Maximum clock / PWM frequency"));
                }
                if output.kind == Kind::Pwm {
                    checks.push(compare(DutyMin, DutyMin, true, "Minimum PWM duty cycle"));
                    checks.push(compare(DutyMax, DutyMax, false, "Maximum PWM duty cycle"));
                }
                if output.kind == Kind::Rf {
                    checks.push((Status::Manual, "Review frequency, impedance matching, RF power, loss, and noise.".into()));
                }
                if output.kind == Kind::Contact { checks.push((Status::Manual, "Review wetting current, debounce, and contact ratings.".into())); }
                let mut automatic = Vec::new();
                if output.kind.logic() && matches!(output.drive, Drive::PushPull | Drive::TriState) {
                    automatic.extend([HighMin, LowMax, SourceCurrent, SinkCurrent]);
                }
                if matches!(output.kind, Kind::AnalogVoltage | Kind::DifferentialAnalog) {
                    automatic.extend([VoltageMin, VoltageMax]);
                }
                if output.kind == Kind::AnalogCurrent { automatic.extend([CurrentMin, CurrentMax]); }
                if output.kind.differential() { automatic.extend([CommonMin, CommonMax]); }
                if matches!(output.kind, Kind::Clock | Kind::Pwm) { automatic.extend([FrequencyMin, FrequencyMax]); }
                if output.kind == Kind::Pwm { automatic.extend([DutyMin, DutyMax]); }
                if output.values.keys().chain(input.values.keys()).any(|key| !automatic.contains(key))
                    || !output.additional.is_empty() || !input.additional.is_empty()
                    || !output.power_off_behavior.is_empty() || !input.power_off_behavior.is_empty()
                {
                    checks.push((Status::Manual, "Additional ratings and timing parameters require engineering review.".into()));
                }
                if output.drive == Drive::TriState {
                    checks.push((Status::Manual, "Review output-enable timing and bus contention for the three-state driver.".into()));
                }
            }
        }
        _ => checks.push((Status::Open, "Set signal characteristics on both ports to compare them.".into())),
    }
    let status = if checks.iter().any(|c| c.0 == Status::Fail) { Status::Fail }
        else if checks.iter().any(|c| c.0 == Status::Open) { Status::Open }
        else if checks.iter().any(|c| c.0 == Status::Manual) { Status::Manual }
        else { Status::Pass };
    let summary = if source.signal_spec.is_none() && target.signal_spec.is_none() {
        checks[0].1.clone()
    } else { "Declared interface checks. See the individual results and test conditions.".into() };
    (status, summary, checks.into_iter().map(|(state, detail)| format!("{} · {detail}", state.label())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PortKind;

    #[test]
    fn logic_failures_override_other_passes() {
        let mut output = Port::new(1, PortKind::SignalOut);
        let mut input = Port::new(2, PortKind::SignalIn);
        output.signal = "Enable".into();
        input.signal = "Enable".into();
        let mut spec = Spec { reference: "GND".into(), drive: Drive::PushPull, ..Spec::default() };
        spec.values.extend([
            (Parameter::HighMin, 3_000_000), (Parameter::LowMax, 200_000),
            (Parameter::SourceCurrent, 4000), (Parameter::SinkCurrent, 4000),
        ]);
        output.signal_spec = Some(spec.clone());
        spec.values.extend([
            (Parameter::HighMin, 2_000_000), (Parameter::LowMax, 800_000),
            (Parameter::SourceCurrent, 10), (Parameter::SinkCurrent, 10),
        ]);
        input.signal_spec = Some(spec);
        assert_eq!(evaluate(&output, &input).0, Status::Pass);
        output.signal_spec.as_mut().unwrap().values.insert(Parameter::VoltageMax, 3_300_000);
        assert_eq!(evaluate(&output, &input).0, Status::Manual);
        output.signal_spec.as_mut().unwrap().values.insert(Parameter::HighMin, 1_800_000);
        assert_eq!(evaluate(&output, &input).0, Status::Fail);
    }

    #[test]
    fn bipolar_signal_values_retain_sign_and_precision() -> Result<()> {
        assert_eq!(parse("-10.25", 6)?, Some(-10_250_000));
        assert_eq!(parse(".0", 6)?, Some(0));
        assert_eq!(number(-10_250_000, 6), "-10.25");
        let mut spec = Spec { kind: Kind::AnalogVoltage, ..Spec::default() };
        spec.values.extend([(Parameter::VoltageMin, -10_000_000), (Parameter::VoltageMax, 10_000_000)]);
        validate(&spec)?;
        spec.values.insert(Parameter::Impedance, -1);
        assert!(validate(&spec).is_err());
        Ok(())
    }
}
