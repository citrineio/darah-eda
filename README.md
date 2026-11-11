# Darah EDA v0.7.3

Darah is a local desktop workspace for defining electronics systems as functional blocks. It checks declared interface compatibility and shows how each load contributes to a rail budget. The engineering model lives in Rust, and the desktop UI uses egui with eframe.

Darah 0.7.3 is an updater validation release for testing download progress, the optional stable AppImage filename, and restarting into the new version.

Darah 0.7.2 adds AppImage update progress, an optional stable AppImage filename, and Flatpak packaging.

Darah 0.7.1 restores missing entries in **Help → What's new** and lists the current release first.

Darah 0.7.0 adds **Help → Check for Updates** for configured Linux builds. AppImage updates are verified with TUF and applied after the user confirms. Debian upgrades use a signed APT repository. See [Publishing Darah updates](docs/publishing-updates.md) for GitHub setup and release maintenance.

Darah 0.6.3 stops auto-panning when the pointer leaves the canvas and cancels canvas drags when the window loses focus or the pointer leaves the window.

Darah 0.6.2 adds left-drag block selection, right- and middle-button panning during edits, and edge panning. Click the name or outline of a system or subsystem and press Delete to remove its boundary. Its blocks and wires stay on the canvas.

Darah 0.6.1 uses green for passing rails, red for failures and yellow for rails that need more information or manual review in the PDF and HTML Power Review exports. Verdict labels remain visible in every format.

Darah 0.6.0 adds **Power Review → Export detailed review**. The report captures the selected system or subsystem and active operating mode. PDF and HTML show rail budgets in current and nominal watts, the power path, input provenance and calculation steps. CSV provides the same sections as a calculation register. Project saves still use schema 13.

Darah 0.5.4 remembers common voltage presets per port. Picking a voltage or changing its tolerance immediately fills the bounds. Nominal voltage follows their midpoint unless a manual override is set in Details. The protection editor derives planning values from the connected supply and connector ratings, shows which inputs are missing, and can fill blank fields on existing stages.

Darah 0.5.3 adds dedicated power converter blocks. Choose an AC/DC or DC/DC topology in Add block, set input compatibility on Power input, and configure losses and isolation on Regulated output under Details. The input does not add an independent load. In Canvas, choose **+ System / subsystem** and drag a boundary. Subsystem membership is edited independently of layout in the block inspector. Choose a scope in Design Checks or Power Review. **File → Export review** writes PDF, HTML, CSV or Markdown for that scope.

Darah 0.5.2 adds a nominal DC Watts view in Power Review. The design verdict still uses voltage limits and current budgets. The contributor list separates declared additional input draw from modeled converter demand. A source rating belongs on its output rail.

Darah 0.5.1 lets you drag a horizontal wire segment vertically, a vertical segment horizontally, or a selected wire's corner in either direction. Collinear segments merge automatically. Select a block and drag its lower-right grip to resize in grid increments. **File → Export diagram** and **File → Export requirements** offer PDF, PNG and SVG. Requirements also export to CSV. On Debian and Ubuntu the package installs `librsvg2-bin` for PDF and PNG rendering. `Ctrl+S` saves and shows a confirmation in the toolbar. `Ctrl+Shift+S`, `Ctrl+O`, `Ctrl+N` and `Ctrl+Y` open Save As, Open, New and Redo.

Darah 0.5.0 adds a cursor preview for block placement, a Move ports mode, and Overview and Details views for ports. Leaving an edited port offers Apply, Discard, or Keep editing. The wire inspector identifies the finding behind the canvas color. The toolbar opens Power Review directly and reports upstream demand, including a lower-bound estimate when a likely cascade needs converter details. An external connector can add recommended protection stages from the top pane, using declared connector data. Protected rail voltage follows the supply input by default. Requirements use stable project-local references, and File → Open recent remembers saved and opened projects.

v0.4.5 gives connectors separate signal-family ports and a vertical physical pinout. HDMI presets show TMDS, CEC, I2C and Power on the diagram. Set an edge and slot in Connector properties, or drag the square on a selected port. Existing individual ports remain available with their wiring. The board direction on shared-control families is a planning convention.

v0.4.3 adds Connector, Protection and Signal conversion to the regular Add block type selector, including Add block opened from the canvas context menu. Continue to setup opens the corresponding tools. Conversion creates a channel and correctly typed ports (including a supply input for active circuits). Protection offers series power, series signal or a shunt attached to a selected existing net. Connector opens the catalog with the chosen block name. Save changes commits the draft and selects the new block on the diagram; Discard leaves the project untouched. Existing Interfaces tools remain available. Project schema remains 9.

v0.4.2 makes Design checks compact, with neutral selection shading, a small status accent and matching check numbers between the list and details. Individual results are sorted by severity in both Design checks and the connection Inspector: failures, warnings, unresolved items, manual reviews, then satisfied results; declared inputs follow. The connection Inspector retains its colored result cards. Diagram wires and port dots retain health coloring and selected-wire outlines from v0.4.1. Project schema remains 9.

## Build on Linux

Use current stable Rust and Cargo. These package names are for Debian and Ubuntu. The portal package provides the Open and Save dialogs. A desktop that already has a working file chooser portal can use its existing backend.

```sh
sudo apt update
sudo apt install build-essential pkg-config libgl1-mesa-dev libx11-dev libxkbcommon-dev libwayland-dev xdg-desktop-portal xdg-desktop-portal-gtk
rustup update stable
cd darah-eda
cargo +stable run --release
```

Run from a graphical desktop session. If Cargo is already managed by your distribution and current, use `cargo run --release`. To build without launching:

```sh
cargo +stable build --release
./target/release/darah-eda
```

Cargo resolves and downloads dependencies on the first build and creates `Cargo.lock`. Keep that lockfile with the project once you have a working dependency set. Darah itself works offline once built. This source handoff targets Linux desktop use. The native window and file picker have not been qualified on other platforms.

If the file picker fails under a Wayland compositor, check that the session has a file chooser portal, such as the GTK backend above. The wlroots portal alone does not implement that interface. An existing project can also be opened directly:

```sh
./target/release/darah-eda /path/to/design.darah.toml
```

## Build the Linux release packages

To test and build AppImage and Flatpak together, install Flatpak and
Flatpak Builder, add the Flathub user remote, and run:

```sh
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
python3 scripts/release.py build
```

The release helper writes both artifacts to `target/dist/VERSION/`.
The legacy Debian build scripts remain available for local use.

To build an AppImage, download the linuxdeploy AppImage for your build architecture from the [official linuxdeploy releases](https://github.com/linuxdeploy/linuxdeploy/releases), make it executable, and run:

```sh
LINUXDEPLOY=/absolute/path/to/linuxdeploy-x86_64.AppImage bash scripts/build-appimage.sh
```

Use the aarch64 tool on an ARM64 build host. The output is placed in `target/appimage/`. Choose a build distribution compatible with your intended test machines. Test the graphical window and file chooser on each supported environment.
Release downloads include the version in their name. Before creating a desktop
shortcut, you can give your downloaded copy the stable name
`Darah-EDA-x86_64.AppImage` (or `Darah-EDA-aarch64.AppImage`). Updates keep the
current path by default and can optionally switch to the stable name.

To build and test Flatpak on its own:

```sh
bash scripts/build-flatpak.sh
flatpak install --user ./target/flatpak/darah-eda-0.7.2-linux-x86_64.flatpak
flatpak run io.github.citrineio.DarahEDA
```

Use Darah’s Open and Save dialogs to access projects outside the sandbox.
Flatpak installations from a configured remote use that remote for updates.
A standalone local bundle is useful for testing and can be upgraded by
installing a newer bundle.

## Start with the guided example

1. Click **Start guided monitor example**. The baseline rail check should show **210 mA used / 300 mA available**, with **90 mA margin**. Its details list the 35, 160, and 15 mA loads.
2. Click **Add alert output**, then **Preview alert → 3V3**. The preview should show a **20 mA shortfall**. Click **Connect ports** to add the link to the draft and inspect the failed check.
3. In the Inspector, set **Available current (mA)** to **350** and click **Apply port details**. The rail should show **30 mA margin**. The alert control port should still be unresolved.

Use **Help → Help topics** or **F1** from any workspace or the welcome screen. Search the offline help by topic or a word inside the instructions. Separate pages cover blocks and ports, connections, power and batteries, requirements, checks, files, keyboard shortcuts, and recent changes. Escape closes Help or About. To restart the example, choose **File → Load environment monitor**. Loading another project prompts you to save if you have edited the current one.

**Help → About Darah EDA** shows the compiled application version, release/debug build, operating system, architecture, readable project schemas, current save schema, and implementation stack. **Copy build info** copies those details to the clipboard for issue reports. Version information comes from Cargo package metadata, so it follows the executable you actually launched.

## Continue the acceptance run

4. Select the alert's supply input and clear **Maximum load (mA)**, then apply. The rail budget should become unresolved with **210 mA known**. Change the rail limit to **200 mA** and confirm that it fails despite the unknown alert load. Use Undo to restore the previous states.
5. To exercise voltage compatibility separately, set the rail's output maximum to **3550 mV** and apply. The affected voltage checks should fail against the loads' **3500 mV** accepted maxima, while the declared loads still appear in the current budget.
6. Save to a new `.darah.toml` file, reopen it, and export a Markdown review. Rename a block and check the TOML diff. Its ID and bindings should be preserved. Saving the unchanged project again should keep its bytes identical.
7. With a saved file still open, append a TOML comment from another editor. Make a Darah edit and click **Save**. Darah should report the external change. Use **Save As** with a fresh filename to keep your edits.
8. Delete a bound rail or its block. Confirm that the corresponding required load ports reopen and that Undo restores the bindings.

## Start your own design

Use **Start a blank project** or **File → New**. Edit the device purpose in the Inspector. Choose **Add block** above the canvas and keep **Include starter ports** checked to begin with editable ports. To add another port, select the block on the canvas and choose **Add port**. Open **Requirements** to create a requirement or a system goal. A block’s Inspector has a shortcut to start a block-owned requirement there.

The inspector commits changes on **Apply**, **Create**, or **Save requirement**. Switch selection after committing a draft. File saves and exports contain committed edits. Save As and review export require a fresh filename. Undo and redo cover up to 64 project edits in the current session.

## Work on the canvas

- **Connect ports.** Drag from either port dot to the other. Green marks a structurally valid target. Review the signal or power checks in the preview, then choose **Connect ports**. You can also select a port and choose **Connect…** to search for a target.
- **Adjust a connection.** Drag a visible section of wire. Click a wire to select it and reveal its round handle. You can position one routing point per wire, and Darah draws right-angle segments through it. The route is saved with the project, and the ports stay connected. Use **Reset route** to restore automatic routing, or **Disconnect** or **Del** to remove the link. Undo restores either change.
- **Move around.** Drag blocks to reposition them. Drag with the right or middle mouse button to pan, including during a left-button drag. Holding the pointer near a canvas edge during a drag pans in that direction. Scroll to zoom and choose **Fit view** to bring the diagram into view.
- **Make room.** Use **View** to toggle the outline and Inspector. The **Design checks** tab shows how many findings need attention. Select a finding, then open its requirement or show it on the diagram. **Review** opens operating modes and the power budget.

## Operating modes and power review

- Choose **File → Load operating modes demo** to open **Power review**. The same 300 mA rail shows 90 mA of margin in **Idle**, a 20 mA shortfall in **Alert active**, and an unresolved budget in **Alert draw unknown**.
- Choose **Manage modes in the toolbar** to add, rename, or delete an operating mode. The base mode is named in the project Inspector. Switch modes in the toolbar to change which current values the checks and connection previews use.
- Select a rail or load port to edit the mode current in the Inspector. **Use base value** inherits the port's ordinary current, **Mark unknown** overrides it with an unknown, and entering zero means the port draws or supplies zero current in that mode. The current unit selector applies to both current fields.
- Click a Power review row to select its mode and inspect the affected rail and load contributions. **File → Export review…** includes a table covering every rail in every mode.
- **Ctrl+K** searches commands, blocks, and ports. Choose a result to center the owning block on the canvas and open the selection in the Inspector.

Voltage and connectivity checks apply across modes. Operating modes are independent snapshots of declared current limits and loads. The active mode is a view setting and resets to the base mode when opening a project. Older projects open with their existing base values. Saving writes schema 13. Use **Save As** to retain a separate file for older builds.

## Voltage and source catalog

Power ports now declare **DC** or **AC RMS**. DC has an explicit positive/negative polarity. AC has frequency bounds, one or three phases, and a line-to-neutral, line-to-line, or transformer-secondary voltage reference. The numerical voltage fields contain positive magnitudes.

Use the two independent dropdowns in the source catalog. **Source category** stays visible and selects regulated DC, bench/programmed supplies, USB/PD, AC mains, transformer secondaries, vehicle/aircraft/telecom, PoE/energy sources, or batteries. **Source preset** lists only profiles from that category. Choosing a preset fills its editable defaults. Changing category clears the previous preset selection and keeps the existing numeric values until you choose another preset. **Custom values** keeps manual entry available within a category; **Custom source** disables the preset list for fully manual entry. The category filter is an editor setting; a saved preset restores its category when reopened.

The source selectors fill the Inspector width, with larger rows and a maximum list height of 360 pixels. Long lists display their option count and a scrolling hint. Overflow scrollbars stay visible without hovering. The Inspector is wider and can be resized between 300 and 560 pixels, with a permanently visible vertical scrollbar.

A selected source fixes its AC/DC type and DC polarity. Batteries, USB/PD, regulated DC, vehicle DC, PoE, and solar sources show DC. AC mains, transformer secondaries, and AC generator profiles show AC RMS. The supported converter channels also fix their output to positive DC. The fields for nominal voltage, bounds, current, and AC operating characteristics remain editable. Choose **Custom source** for a different independent-source electrical type or polarity. The common-voltage shortcut is shown only for custom power definitions.

Common regulated outputs include 3.3, 5, 6, 7.5, 9, 12, 13.5, 15, 18, 19, 19.5, 20, 24, 27, 28, 30, 36, 42, 48, 54, and 60 V. Custom nominal values and bounds remain available.

DC supply entries start with editable ±5% planning bounds. AC mains/secondary entries start with editable ±10% voltage bounds. Those are generic assumptions, and source current remains unknown until declared. Solar and generator entries require their actual voltage envelope. Vehicle and aircraft entries cover a planning operating envelope. Record cold crank, load dump, surge, and other transient requirements separately.

An AC rail matches a load only when voltage, frequency, phase count, reference, and power domain agree. A DC rail also matches polarity. AC current fields are RMS envelopes. Three-phase budgets require manual per-phase loading review. Buck, boost, buck-boost, and LDO channels use positive DC. A single-phase AC/DC channel uses declared input power factor and efficiency. Its upstream RMS current is derived from output power, input RMS voltage, efficiency, and power factor.

USB-C PD sources start with unconfirmed contracts. **Assume negotiated contract is available** records a design assumption. The affected path stays open until confirmed. For AVS, declare the negotiated operating envelope or retain the full variable range if the load must support it.

## Battery capacity

Choose a battery source, then a reference cell/size template and **Series / Parallel** layout. Capacity is for the complete pack and can be entered in **mAh** or **mWh**, including decimal values. Switching units uses the declared pack nominal voltage. Blank capacity remains unknown.

Under **Batteries**, the second dropdown selects **Battery chemistry**. **Cell format / reference** then lists only cells from that chemistry. Li-ion/LiPo offers 18650, 21700, and pouch examples; NiMH offers AA and AAA. LiFePO₄, alkaline, lead-acid, lithium coin, primary lithium, silver oxide, and zinc-air stay in their own families. Changing a cell format keeps its chemistry selected in the second dropdown.

Defaults describe specific reference cells or explicitly labeled illustrative sizes. For example, the reference Li-ion 18650 default is 2800 mAh, the reference 21700 default is 4500 mAh, NiMH AA is 1980 mAh, and CR2032 is 225 mAh. These are editable starting values. Chemistry alone does not specify a capacity. Changing the cell template or S/P layout reloads its reference default. The selected series count scales voltage. Parallel count scales charge capacity. Nominal stored energy uses pack capacity and nominal voltage. It is a planning estimate and does not model voltage curves, load-rate effects, usable state of charge, aging, or runtime.

A 2S2P pack using 2800 mAh, 3.6 V cells starts at 7.2 V and 5600 mAh, equivalent to 40320 mWh nominal. Capacity does not set a safe discharge-current limit.

## Signal characteristics

Keep the interface contract name on both ports. Enable **Electrical signal characteristics** to choose digital, analog voltage, analog current, clock, PWM, differential digital/analog, contact, or RF.

The forms capture logic thresholds, source/sink current magnitudes, output stage, absolute ratings, reference/domain, test conditions, unpowered behavior, bipolar analog ranges, compliance, common mode, impedance, timing, pull networks, noise, accuracy, acquisition rate, and resolution. Additional parameters accept name, value, unit, and conditions/source.

**Add interface fields** offers I²C, SPI, UART, CAN/CAN FD, RS-485, USB/high-speed serial, audio/I²S, RF, and contact templates. Those create named fields for you to fill. Protocol-specific fields remain subject to engineering review.

Automatic checks compare declared push-pull/three-state logic levels and drive-current capacity, analog signal ranges, differential common mode, clock/PWM frequency, and PWM duty range. Missing core values stay open. Captured timing, absolute ratings, impedance networks, protocol settings, and other custom parameters remain manual review. Interface checks use declared conditions and do not perform waveform, signal-integrity, or protocol simulation. Existing projects with contract-only signals retain their original check behavior.

The research basis and the full field matrix are in [docs/interface-and-source-design.md](docs/interface-and-source-design.md).

## Find missing rails

**Design checks** inspects required supply inputs. It distinguishes a missing range, an available voltage match needing a connection, a match awaiting PD, and a missing compatible rail. It lists incomplete existing rail definitions instead of assuming those rails are incompatible. Proposed ranges are intersections of accepted input ranges.

Inputs with the same explicitly declared nominal voltage and matching **Power domain** names can appear in a potential shared-rail proposal. Domains are optional. When you name them, use the same name on the rail and its intended inputs. Give separate domains to inputs that must remain separate. Existing downstream rails are excluded as candidate feeders when the connection would create a loop.

Select a finding and choose **Show on diagram** to edit or connect the input. Review current capacity before connecting a voltage match. Suggestions leave the design editable and require an engineering decision before creating a rail or converter.

## Define a power topology

1. Create a block for the converter or PMIC. Add each physical input as a **Supply input** and each output as a **Supply rail**. Set the voltage bounds and rail current limits, then connect the external supply and loads.
2. Select an output rail. Under **Power topology**, choose **Buck converter** or **LDO regulator**. Set **Feeder on this block** to a supply input or to another rail on that same block. Enter the buck efficiency or the LDO ground current and maximum dropout, then click **Apply power topology**.
3. Repeat for each output channel. Channels can share a feeder. Use an internal rail as the feeder for another channel on the same PMIC. Darah rejects circular paths. **Independent source** removes the conversion relationship.
4. Open **Power Review in the toolbar** for the per-mode budgets. Select a row to see its contributing loads and converter channels in **Design checks**. Markdown export includes the topology and its assumptions.

The budget estimates steady-state input draw. A buck uses the supplying rail's minimum voltage, the output rail's maximum voltage, and the declared efficiency. Use an efficiency appropriate for the modeled conditions. The LDO adds ground current to the downstream load and checks the declared dropout separately. Channels are treated as enabled in every operating mode. Unknown inputs remain visible, and a known subtotal can still exceed a limit. For a converter feeder, any declared current on its **Supply input** adds to the calculated demand as auxiliary draw. Leave that field blank to add zero. Inrush, sequencing, and thermal limits require separate verification.

Try **File → Load cascaded power example**. With 300 mA on 3.3 V and 100 mA directly on 5 V, Power review shows **320 mA on 5 V** and **148.149 mA on 12 V**. If you set the 3.3 V load to zero, the upstream figures become **100 mA** and **46.297 mA**. **File → Load PMIC example** shows two bucks on shared VIN and an LDO supplied by the internal 5 V rail. The example produces **400.1 mA on 5 V** and **218.566 mA on 12 V**.

## Requirements workspace

**File → Load desk monitor specification example** opens **Requirements** and selects the standby power requirement. The list brings together system goals, block requirements, and electrical requirements. Select a row to see or edit its source, assignment, and verification method. The design check uses saved inputs. Verification evidence has a separate status and field.

Use **New requirement** to define a requirement and assign it to a block, port, connector pin, or transition. **New system goal** supports manual verification with multiple responsible blocks. Connector pins, control states, and transitions are edited in their own tabs within the workspace. Select **Design checks** to review open or failed calculations and jump to their inputs.

**File → Export review…** writes the requirements and verification matrix, pin assignments, and control tables from the project. Calculation results use declared inputs. A design pass leaves hardware verification pending.

Try these changes on a copy of the example:

1. The declared 4.5–5.5 V input covers the requirement allocated to connector pin 1. In **Connector pins**, unmap pin 1 and save the entry. The linked voltage check becomes open, and the next export shows the changed pin mapping.
2. Restore the mapping. Open **Standby input power**, then click **Open Monitor power input / 5 V DC input**. In the port Inspector, enter **10 mA** under **Maximum load** and click **Apply port details**. Click **Back to requirements**. The standby design check passes at **0.05 W** and verification evidence stays **Pending**. The active check uses its own operating mode.
3. Open the Button pressed transition. Its blank guard is an open decision. The transition editor captures conditions and actions as text for engineering review.

Saving writes schema 13. Earlier projects remain readable. Use **Save As** if an older build still needs access to the original project file.

## Grid and alignment

- Use **Grid** above the diagram to show or hide the grid, choose spacing of 10, 25, 50, or 100 canvas units, and toggle snapping. The default spacing is 25. Snapping uses the block's top-left corner or the wire's routing point. Grid settings are kept for the app session, and block positions save with the project.
- Click the header of the block you want to keep in place. **Shift-click** the headers of the other blocks. The reference block has a purple border. Use **Align** to match an edge or center, then use Undo to reverse the whole alignment in one step.
- Alignment follows the reference block even when an edge or center is off the grid. Use **Grid → Snap selected to grid** when you want the selected top-left corners on grid intersections.

## Edit groups of blocks

- Right-click a block in the diagram or outline to **Delete block** or **Duplicate block**. You can also select block headers and use the buttons above the diagram or the **Del** and **Ctrl+D** shortcuts. Deleting blocks removes their ports and connections. One Undo restores them together.
- Left-drag from empty space to box-select blocks. **Shift-click** a block to add or remove it from the selection. Drag a selected block to move the group. For three or more blocks, **Distribute** evens the gaps along one axis while keeping the outer blocks fixed.
- **Duplicate** copies the blocks, their ports, their block-owned requirements, and connections between selected blocks. Links to blocks outside the selection stay on the originals. Copied requirements retain responsibility links only within the copied group.
- Double-click a port to open the connection picker.

For a quick verification, duplicate **Temperature sensor** and **Controller** together in the sample. Their signal connection should copy, while the copies still need supply connections. Delete the copies and undo once to restore both. The model regression checks for these operations are available with `cargo +stable test`.

## Check the v0.1.2 changes

- Drag a block, undo the move, then drag it again. Save the project and reopen it to confirm the position is retained. Right-click the moved block and choose **Reset block position** to restore automatic layout.
- Right-click a block and add a port. After creating it, right-click the port row on the diagram and choose **Connect this port…**.
- On a power port, switch voltage from mV to V and current from mA to A. An existing 3250 mV value should display as 3.25 V, and 300 mA as 0.3 A. Change a load to 250 µA and verify that it displays as 0.25 mA when you switch back. The budget still uses the full precision.
- Open an older project and confirm the same current budget. The app reads schema 1 through 13 projects and writes schema 13 when you save. Use **Save As** if you want a file you can still open in the older build alongside the upgraded file.

## v0.1 check scope

Signal bindings use shared interface contracts or legacy names. One signal driver may feed multiple inputs, with aggregate current and capacitive loading reviewed separately. Multi-driver and bidirectional protocol behavior is not simulated. A supply input can bind to one rail and a rail can serve multiple loads. Voltage checks compare the complete declared intervals. Current checks sum declared load maxima in the selected operating mode. Missing values stay visible, and a known subtotal can already establish a failure. Device goals retain their responsible blocks and manual verification methods.

The TOML file is the source of truth. The Markdown review includes a fingerprint of the serialized project. This milestone covers system definition and review. Component selection, schematic and PCB editing, and physical validation are subsequent work.

## Interfaces and block tools (v0.4.0)

Use the **Interfaces** menu for shared contracts, bulk ports/templates, signal conversion, connectors, and protection. Block Inspector shortcuts open the matching editor tab. **Save changes** commits the editor draft; **Ctrl+S** then saves the project file. See [docs/interfaces-v0.4.md](docs/interfaces-v0.4.md) for a complete walkthrough, catalog scope, assumptions and acceptance checks.
