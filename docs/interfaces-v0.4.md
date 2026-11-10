# Interfaces and block tools — v0.4.0

Open **Interfaces** in the main toolbar. A block's Inspector also opens its port, conversion or protection tools. This editor has five tabs and works on a draft of the project. **Save changes** applies the draft as one undoable project edit. **Ctrl+S** then saves the project file. **Discard and close** cancels the draft. The background project is disabled while this editor is open.

Since v0.4.3, **Add block** also lists **Connector**, **Protection** and **Signal conversion**. These entries open the same setup tools with the supplied block name. Conversion initializes its first channel, signal ports and any active supply input. Protection offers series power, series signal or a shunt attached to a chosen existing port/net. Connector opens its catalog; create the selected preset before saving. **Save changes** commits the draft and selects the new block on the diagram. **Discard and close** leaves the original project intact.

## Shared signal contracts

A contract describes an interface requirement. A port's signal characteristics describe the actual hardware. Selecting a contract does not change those hardware limits.

1. Add your processing block and a signal input for its ADC. Apply its real analog input range and other electrical characteristics in the Inspector.
2. Click **Create shared contract from applied port details…**. Name the contract, for example `Ambient sensor voltage`. Keep interface limits such as the intended voltage envelope and reference domain. Converter resolution and acquisition details remain hardware properties.
3. Save changes. Select the sensor output, enter its actual output characteristics, and choose the same contract in the shared-contract selector. Apply port details.
4. Connect the output to the input. Design checks compare both actual endpoints and each endpoint against the shared requirement.

You can begin with the sensor instead. Contracts belong to the project and can be selected from any signal port. Editing a shared definition updates every linked port's review; linked ports are listed before editing. Duplicate a contract when an interface needs an independent definition. Deletion is blocked while ports or saved port templates still use it.

For an example, a 0–5 V sensor assigned a 0–3.3 V contract remains a 0–5 V sensor and fails that requirement. A conversion stage is needed. Assign separate contracts to each side of that stage.

Voltage, current, frequency and duty limits describe an envelope. Sources must produce values within it; sinks must accept it. Logic high/low constraints follow source guarantees versus sink thresholds. Bandwidth and data rate are minimum capabilities. Fields left unknown produce open checks where needed; a name alone cannot establish compatibility.

## Many ports and reusable templates

In **Ports and templates**, select the owning block. Choose a fresh port type, name prefix, first number and count, then **Create numbered ports**. Up to 1,024 ports can be created at once. Optional physical pin numbering uses the same starting number. Logical names and physical pin numbers are separate.

Configure one port's actual characteristics in the Inspector, apply them, and save it as a template. Use that template when creating more ports, or select existing ports and choose **Apply chosen template to selected**. Template application copies hardware fields while preserving the existing IDs, names, physical pin numbers and edge placement. Connections retain their endpoint IDs, but changing types can invalidate them: disconnect affected wires before changing a connected port's direction or signal/power class.

Multi-selection also supports deletion, duplication and shared-contract assignment. Assigning a contract changes requirements only. Duplicating a port copies its characteristics but clears physical numbering and custom placement; it creates no wires. Duplicate physical pin assignments generate a review warning so alternate functions can be checked.

## Signal conversion

In **Signal conversion**, create a conversion block or select an existing owning block, choose an implementation and **Add channel to selected block**. A channel has real input and output ports. Multiple channels can share a block, including a processing device.

Available implementations include divider, gain/offset, buffer, level translator, inverter, comparator, ADC, DAC, current-to-voltage, filter, isolation, protocol bridge and custom. Both sides retain their own signal type and contract. An active channel needs selected, connected supply input(s); use **Add supply input to this block**, then configure and connect those inputs after saving the editor.

Offset is an intentional signed conversion parameter, with a separate nonnegative offset error bound. Use a separate block when the offset circuit is a physically separate stage. A divider needs both resistances, resistor tolerance, intentional offset and offset error explicitly declared; use explicit zero where appropriate. A linear stage also needs its signed gain.

After connecting an upstream source with a known output envelope, **Calculate and set output voltage envelope** calculates a conservative static bound for a divider or linear stage. Resistor tolerance is included for dividers. Negative input voltage and negative linear gain are supported. The result updates the channel output's actual voltage envelope and is checked against downstream acceptance limits.

Bandwidth, resolution, sampling/update rate, reference and assumptions can also be recorded. This is system-level capture: ADC/DAC code transfer, acquisition settling, impedance loading, amplifier headroom, noise, filtering, isolation barriers and protocol behavior need engineering review. The static calculation does not simulate them.

## Draggable ports

Enable **Edit ports ·25-unit grid** above the diagram. Drag a port dot to any edge of its block. It snaps to a 25-unit local grid; connections follow the port's stable ID. Occupied explicit slots advance to the next free slot. Blocks expand to fit their ports.

The Ports tab also provides a **Custom** placement checkbox, edge selector and numbered slot. Clear Custom to return to automatic placement. Disable Edit ports to resume dragging connections. Top/bottom labels show short names or physical pin numbers; hover a dot for the full name.

## Connector catalog and pin mapping

The catalog contains 77 presets in 11 categories: USB, audio/video, networking, headers/ribbon, wire connectors, flexible cables, board-to-board, cards/expansion, power, industrial/RF and custom/test. Select category and family/variant independently. Enter a count for variable-contact templates and choose an initial flow direction before creating a connector.

Some templates provide named logical contacts. Generic worksheets deliberately leave roles unassigned. Enter the exact manufacturer part number, contact numbers, keying, orientation, mating part/cable and datasheet revision. Family membership does not establish part-specific ratings or supported protocols. A USB-C shape does not establish a power-delivery profile.

Each physical contact can map to an existing logical port. Signal and power contacts need mappings for electrical review. Ground, shield, neutral/DC return and NC can be represented without signal ports. Marking a contact as Power does not silently change an existing signal port: configure its logical type in the Ports tab. A mismatch produces a warning. Mixed-direction connectors need per-port direction configuration after creation; the initial direction is a convenience, not bidirectional bus modeling.

Working voltage and per-contact current ratings remain unknown until you enter the actual part data. Add cable gauge/length/derating, shield/chassis/return strategy and hot-plug expectations. Save a configured connector as a reusable project template; instantiation creates fresh ports and clears live mappings and protection exceptions.

The catalog does not certify mechanical mating, pin mux availability, parallel-contact current sharing, cable ampacity or protocol compliance. USB-C repeated power contacts are separate physical contacts; their current capacity is not automatically pooled.

## Protection and design review

Create a protection block, then add a **shunt** definition or **series power/signal protection with ports**.

- Shunt protection attaches to a deliberately selected existing port/net and records a discharge/return reference. The diagram draws a thin amber attachment line. The attachment represents metadata, not a serial signal or power connection.
- Series protection uses its own input and output ports. Connect the upstream source to its input and its output to the downstream load. Series power paths carry downstream current demand upstream; additional device current is the input's auxiliary draw. Declare voltage intervals on both sides. Series resistance/drop and trip dynamics are recorded for manual review, not simulated.

Select the faults covered: ESD, surge, EFT/burst, overvoltage, overcurrent/short circuit, reverse polarity, reverse current/backfeed, inrush/hot plug, inductive discharge, overtemperature, isolation/bonding and EMI filtering. Record event waveform/level/polarity, standoff, clamp voltage at a stated current, current limit, response time, capacitance, leakage, series resistance, evidence and conditions as applicable.

Connector checks operate per mapped signal/power contact. External power contacts additionally request overvoltage/overcurrent coverage and hot-plug coverage when selected. Declared coverage must attach to that contact's connected net. A clamp beyond a conversion stage does not cover the upstream connector net. Series elements can cover their entry-side net when declared there.

Missing coverage is a warning. Declared coverage remains manual review because placement, return impedance, bypass paths, event conditions and coordination matter. Checks also flag incomplete ratings, normal voltage above standoff, clamp above declared absolute maximum, connector current/voltage ratings, overcurrent coordination, physical-role mismatches and declared protection capacitance exceeding the source budget. Current, capacitance and timing need complete endpoint data; signal fan-out checks aggregate declared endpoint loading.

For an intentional exception or externally provided protection, enter a reason **and select the specific faults in Exception fault scope**. A reason for ESD does not waive overcurrent. Exceptions remain visible as manual review, including in exported reports.

## Files, migration and local verification

This build reads project schemas 1–9 and saves schema 9. Open an existing project and use **Save As** to retain a copy for an older executable. IDs, contract references, pin maps, templates, channel definitions, protection definitions and port placement persist in the project. **File > Export review** includes the new definitions and check evidence.

From the source directory, the normal local checks and build are:

```bash
cargo +stable test
cargo +stable run --release
```

For the existing Debian packaging workflow:

```bash
bash scripts/build-deb.sh
sudo apt install ./target/debian/darah-eda.deb
```

The packaging script retains its saved maintainer identity and existing optional logo integration. This update changes no logo assets.

Suggested app checks after compilation:

1. Open an older project and save a separate copy; reopen it and confirm existing blocks/wires.
2. Link a 0–5 V output and 0–3.3 V input to a 0–3.3 V contract. Confirm the output still reports its real limits and a failed check.
3. Add a passive divider with 10 kΩ/10 kΩ, 1% tolerance and explicit zero offset/error. Connect the 0–5 V source; calculate its output and confirm its upper bound is greater than 2.5 V but below 3.3 V.
4. Create 32 numbered ports from a saved template. Move several to different edges, save/reopen, then duplicate and delete the block. Check that no live reference is left dangling.
5. Add an external connector. Confirm missing protection warnings; attach a shunt definition to its net and confirm coverage becomes manual review. Move the attachment across a conversion stage and confirm the original warning returns.
6. Insert series power protection in an existing supply path. Confirm downstream demand still appears in the upstream budget. Try a return path to create a loop; validation should reject it.
7. Export a review and check contracts, contact maps, scoped exceptions, conversion parameters and protection evidence.

## Engineering references

These sources informed the model and its review boundaries. Use the selected component's current datasheet and the applicable standard revision for an actual design.

- [TI, ADC input drive and settling](https://www.ti.com/document-viewer/lit/html/SNAA363/GUID-7B0D26D5-46C0-4D6F-94BC-9579FEFB9A94)
- [Analog Devices, AN-2555](https://www.analog.com/en/resources/app-notes/an-2555.html)
- [TI, ESD device selection](https://www.ti.com/document-viewer/lit/html/SLVAFQ4/GUID-E87DB142-562D-408B-802B-30488CFE68C5)
- [TI, ESD/EFT/surge protection](https://www.ti.com/document-viewer/lit/html/SLVAFR3)
- [Analog Devices, system power protection](https://www.analog.com/en/resources/analog-dialogue/raqs/raq-issue-216.html)
- [Samtec, current carrying capacity and conditions](https://blog.samtec.com/post/current-carrying-capacity/)
- [USB-IF document library](https://www.usb.org/documents?search=connector)
- [TE, PCB connector families](https://www.te.com/en/products/connectors/pcb-connectors.html)
- [Amphenol, I/O connectors](https://www.amphenol-cs.com/connectors/input-output.html)
- [JST, connector families](https://www.jst-mfg.com/product/index.php?lang=2)
- [PCI-SIG, M.2 specification overview](https://pcisig.com/specification-overview/pci-express-m2)
