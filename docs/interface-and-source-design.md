# Power and signal parameter design

## Source categories

| Category | Presets and fields | Review implications |
| --- | --- | --- |
| Regulated DC | Common commercial output voltages, tolerance, current rating, polarity | Actual ripple, regulation, transient limits and derating depend on the selected supply. |
| Bench and programmable | 0–30 V and 0–60 V envelopes, custom setpoint and bounds | A programmable range is distinct from the selected operating voltage. |
| USB and PD | Legacy 5 V, fixed SPR/EPR profiles, EPR AVS envelopes | Capability, cable, and negotiated contract require confirmation. |
| AC mains | Representative 100–690 V entries at 50/60 Hz, one/three phases, RMS voltage reference | Line-to-line and line-to-neutral values must remain distinct. |
| Transformer secondary | Representative 6–24 V RMS secondaries, 45–65 Hz starting interval | Regulation and no-load rise depend on the transformer. |
| Vehicle/aircraft/telecom | 12/24/28 V DC, 115 V/400 Hz AC, negative 48 V telecom | Transients and environmental limits require additional requirements. |
| PoE and energy | PSE Type 1/2/3/4 operating envelopes, custom solar/generator definitions | PoE PD voltage differs from PSE voltage because of cable drop. PV sources need the actual panel envelope. |
| Battery | Chemistry, size/reference cell, series, parallel, editable pack capacity in mAh/mWh | Capacity basis includes load rate, cutoff, temperature, and age. |

The catalog provides broad starting coverage and custom entries. Catalog tolerance values are marked as editable planning assumptions. The app does not infer vendor-specific current ratings from a voltage label.

Commercial supply examples were checked against [MEAN WELL EPS-25](https://www.meanwell.com/Upload/PDF/EPS-25/EPS-25-SPEC.PDF) and [the manufacturer catalog](https://www.meanwell.com/productFast.aspx). AC references were checked against [Schneider Electric's voltage system examples](https://www.se.com/us/en/faqs/FAQ000268512/). [TI's PoE overview](https://www.ti.com/document-viewer/lit/html/SSZT530/GUID-C380D019-4CB4-409D-8526-06DF8E0F00BF) distinguishes PSE/PD envelopes. [Fluke's electrical glossary](https://www.fluke.com/en-gb/learn/blog/electrical/electrical-glossary) distinguishes RMS, VA, W, and power factor.

## Capacity defaults and their basis

| Template | Initial capacity | Basis |
| --- | --- | --- |
| Li-ion 18650 | 2800 mAh | Reference typical capacity for Molicel P28A |
| Li-ion 21700 | 4500 mAh | Reference typical capacity for Molicel P45B |
| NiMH AA | 1980 mAh | Published average for Panasonic BK200AAP |
| CR2032 | 225 mAh | Panasonic nominal capacity at stated conditions |
| Lead-acid cell | 7000 mAh | Reference 7 Ah cell capacity for a six-cell NP7-12 battery at its 20-hour rate |
| Other size/chemistry entries | Labeled illustrative capacities | Adjustable placeholders, not statistical chemistry averages |

The manufacturer sources include [Molicel P28A](https://www.molicel.com/product/inr-18650-p28a/), [Molicel P45B](https://www.molicel.com/product/inr-21700-p45b/), [Panasonic BK200AAP](https://energy.panasonic.com/na/business/products/nickel-metal/standard/models/BK200AAP), [Panasonic CR2032](https://industrial.panasonic.com/cdbs/www-data/pdf2/AAA4000/AAA4000C321.pdf), and [Yuasa NP7-12](https://www.yuasa.co.uk/yuasa/datasheet/index/sku/np7-12/).

[Energizer's AA datasheet](https://data.energizer.com/pdfs/E91_Max_NA.pdf) shows capacity varying with discharge current and cutoff conditions. This is why the interface identifies the reference size and basis. Default capacities remain editable.

## Signal field matrix

| Type | Core fields | Additional useful parameters |
| --- | --- | --- |
| Digital | VIH/VIL on inputs, VOH/VOL on outputs, required/available high/low current, output stage | Leakage, pull network, capacitance, unpowered behavior, supply/temperature/load conditions |
| Analog voltage | Signed signal envelope, input/output impedance | Bandwidth, common mode, source/sink limits, noise, error, absolute limits |
| Analog current | Signed current envelope, compliance range, impedance/burden | Loop supply, active/passive behavior, two/three/four-wire interface, wire-fault behavior |
| Clock/PWM | Voltage thresholds, frequency interval, duty interval | Rise/fall time, jitter, pulse width, delay, setup/hold, skew |
| Differential | Differential signal range, common-mode interval, reference | Termination, load units, cable length, pair skew, fail-safe bias |
| Sampled interfaces | Signal envelope and reference | Sampling/update rate, resolution, settling/acquisition time, anti-alias filtering, noise/error budget |
| RF | Frequency band, impedance, bandwidth | Input/output power, insertion/return loss, gain, noise figure, compression point, connector/DC block |
| Contacts | Voltage/current rating, resistance, minimum pulse | Wetting current, debounce, isolation, material, lifetime |
| I²C | Bus role and address | Pull network, bus capacitance, rise time, speed mode, clock stretching |
| SPI | Role, CPOL, CPHA, bit order, word length | SCLK limit, chip-select polarity and timing |
| UART | Baud rate, data bits, parity, stop bits | Flow control, inversion, level/interface standard |
| CAN/RS-485 | Physical layer, nominal/data rate, termination | Unit loading, stubs, common mode, clock tolerance, sample point, fail-safe behavior |
| USB/high-speed serial | Role, generation, lane count | Differential impedance, loss/skew budget, reference clock, topology |
| Audio/I²S | Format, sample rate, word length, channels | Clock role, polarity, framing, jitter |

Useful fields are drawn from [TI's logic datasheet interpretation guide](https://www.ti.com/lit/an/szza036c/szza036c.pdf), [NXP's I²C specification](https://community.nxp.com/pwmxy87654/attachments/pwmxy87654/nxp-designs/931/1/UM10204.pdf), [TI's RS-485 guide](https://www.ti.com/lit/an/slla272d/slla272d.pdf), [Microchip's SPI modes](https://onlinedocs.microchip.com/oxy/GUID-DE09DA5A-1CBB-49A8-9DA0-B2EB94E57E56-en-US-11/GUID-67490DA2-F73A-42D5-889A-2AF016ADDFE3.html), and [NXP's CAN FD overview](https://www.nxp.com/products/interfaces/can-transceivers/can-with-flexible-data-rate:CAN-FDR).

Analog parameters were checked against [Analog Devices' ADC/DAC terminology](https://www.analog.com/en/resources/technical-articles/types-of-adcs-and-dacs.html), [input settling guidance](https://www.analog.com/en/resources/analog-dialogue/articles/successive-approximation-adcs.html), [industrial I/O modes](https://www.analog.com/en/resources/technical-articles/configurable-inputoutput-modes-for-plc-systems-using-the-max22000-and-max14914a.html), and [RF datasheet parameters](https://www.analog.com/en/resources/technical-articles/understand-wireless-data-sheet-specifications--part-1.html).

Automatic comparisons cover the explicitly modeled static limits. Loading networks, waveform timing, protocol settings, RF behavior, and additional parameters are captured for manual engineering review. Bidirectional and multipoint buses can be documented using interface fields. The current signal binding graph retains one source and one sink per connection.
