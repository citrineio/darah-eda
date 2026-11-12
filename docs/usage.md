# Using Darah EDA

[← README](../README.md) · [Installation](installation.md)

Darah turns a functional architecture into a connected engineering model: blocks describe functions, ports describe interfaces, and connections let you review the assumptions between them. Requirements, power budgets, and exported reviews use that same project data.

## Start with the guided example

1. Launch Darah and choose **Start guided monitor example**. The example gives you a supply, controller, and loads to inspect. You can reopen it from **File → Load environment monitor**.
2. Select a block and inspect its purpose and ports. Select a connected port or wire to see the related interfaces and design checks.
3. Open **Power Review**. Follow the rail's declared capacity and the contributions from its loads.
4. Use **Add alert output** in the guided example and preview its supply connection. Compare the projected budget with the original design before committing the connection.
5. Open **Requirements** to inspect or add a requirement. Open **Design checks** to read findings and their evidence.
6. Choose **File → Save As** to save your own `.darah.toml` copy, then export a review from **File → Export review**.

Use **F1** for searchable in-app help at any point.

## Create a project

Choose **Start a blank project** or **File → New**. Give the design a title and purpose, then build its architecture on the **Canvas**.

Choose **Add block**, select a functional type or preset, and place the block. Input, output, processing, power, and custom blocks describe the role of each function. Starter ports give you editable examples; add more through the selected block's **Add port** action.

A useful starting sequence is:

1. Add the main power source and processing block.
2. Add sensors, outputs, and other loads.
3. Declare the interfaces each block needs or provides.
4. Connect the power tree and signals.
5. Review missing inputs, incompatibilities, and capacity limits.
6. Capture the requirements and assumptions that will guide implementation.

### Apply edits, then save

Forms distinguish draft edits from applied project data. Use **Apply**, **Create**, **Save changes**, or **Save requirement**, as appropriate, to commit a form. Block name and purpose edits apply when the field loses focus.

**Ctrl+S** saves the applied project to disk. Check the save confirmation in the toolbar. Apply pending edits before exporting so the report includes them.

## Navigate and edit the canvas

| Action | Control |
| --- | --- |
| Move a block | Drag the block. |
| Select several blocks | Drag a selection box from empty canvas, or Shift-click to change the selection. |
| Pan | Drag with the middle or right mouse button. |
| Zoom | Scroll over the canvas. |
| Connect ports | Drag a round port dot to another port. Inspect the compatibility preview before confirming a link with outstanding checks. |
| Move a port | Select it, then drag its square placement handle along the block edges. |
| Adjust wire routing | Select a wire and drag a routing handle or segment. |
| Resize a block | Select it and drag its resize grip. |
| Duplicate or delete | Ctrl+D duplicates selected blocks; Delete removes the selected object. Context menus provide related actions. |
| Undo or redo | Ctrl+Z and Ctrl+Shift+Z, or the toolbar buttons. |

Use the layout controls to configure the grid, alignment, and distribution. Dragging near the canvas edge pans the view. A selected wire keeps its status color while gaining a selection outline.

## Define power sources, rails, and loads

A **supply rail** provides power. A **supply input** declares what a block accepts and consumes. Record voltage bounds, available source current, and load demand on the appropriate ports.

Choose a source category and preset for standard supplies or batteries. Use a custom source when the preset's electrical type or assumptions do not fit your design. For batteries, configure chemistry, cell reference, series/parallel arrangement, and total pack capacity. AC values use RMS voltage with the declared frequency, phase count, and reference.

Leave unknown values blank. A blank current means **unknown**, while a declared zero is an explicit zero-load assumption.

### Build a power tree

Add an AC/DC or DC/DC converter with its supply input and output rail. Configure the output's conversion channel and connect the input to the upstream rail. Repeat for cascaded supplies or multiple outputs on one block.

Declare the topology and relevant losses, such as efficiency or linear-regulator ground current and dropout. Capture isolation details where applicable. A converter's output demand is propagated upstream through the model; its input is not an additional independent copy of the downstream load. Record any separate input-side demand deliberately.

### Compare operating modes

Use **Manage modes** to capture states such as standby and active operation, then set current overrides for the relevant loads. Select the active mode before reviewing the detailed results.

**Power Review** shows rail budgets and contributions across operating modes. Its current and nominal DC watts views help you inspect capacity and downstream demand. Review missing assumptions and input provenance alongside the totals. Battery runtime, transient behavior, and physical power-system validation require additional analysis.

## Define signals, connectors, and protection

Use **Interfaces** from the toolbar or Inspector to edit signal characteristics and shared contracts. Describe actual source and input capabilities: assigning a shared contract records an interface agreement, while the hardware fields retain each endpoint's declared capability.

Reuse a port template for repeated interfaces, or duplicate a contract when a variation needs to remain independent. Active signal-conversion channels need an appropriate supply connection.

Connector diagram ports represent logical signal families. Physical contacts are captured separately and mapped to those functions. This keeps a high-level architecture readable while retaining implementation references.

For protection, choose series power, series signal, or shunt protection as appropriate. Attach shunt protection to the actual net and document its return. Capture ratings, fault coverage, evidence, and any exceptions. Placement, grounding, component selection, and compliance remain engineering review inputs.

## Capture requirements

Open **Requirements**, then choose **New requirement**. Record:

- The requirement title, source, and source section.
- Its allocation to the system, a block, or a port.
- A criterion and verification method.
- An evidence reference or the work still needed to obtain one.

Use manual criteria for engineering verification. Supported positive DC input-voltage and input-power criteria can compare declared model values; input-power criteria also need the applicable operating mode and test voltage.

A modeled design result and recorded verification evidence answer different questions. Keep both visible during review. The Requirements workspace also captures connector pin mappings, control states, and transitions.

## Review the whole design or a subsystem

Choose **+ System / subsystem** on the Canvas and draw a boundary to organize the design. Review block membership in the Inspector; a boundary's visual position and its assigned membership are edited separately.

Select the overall system or a subsystem in the review scope controls. A scoped review follows the assigned blocks and crossing interfaces. Deleting a boundary removes the grouping while preserving its blocks and wires.

### Read the findings

| Result | Meaning |
| --- | --- |
| **Failed** | A declared limit or modeled compatibility condition is violated. |
| **Warning** | A condition needs attention; read the specific finding before deciding how to address it. |
| **Unresolved** | A required value, connection, or assumption is missing. |
| **Manual verification** | Engineering assessment or external verification is required. |
| **Satisfied within scope** | The declared inputs pass that modeled comparison within its stated scope. |

Select a finding to inspect its details and evidence. Findings are ordered by severity. Wires and port dots show the worst relevant finding; a shared endpoint can reflect another branch's result.

PDF and HTML reviews use **red for failures, green for satisfied results, and yellow for warnings, unresolved items, and manual verification**. Written verdicts remain visible. The on-screen view uses additional status accents, so use the labels to interpret each result.

## Export and share

Apply the current edits and choose the scope and operating mode before exporting.

| Export | Where to find it | Formats |
| --- | --- | --- |
| Block diagram | **File → Export diagram** | PDF, PNG, SVG |
| Requirements | **File → Export requirements** | PDF, PNG, SVG, CSV |
| Design Review | **File → Export review** | PDF, HTML, CSV, Markdown |
| Detailed Power Review | **Power Review → Export detailed review** | PDF, HTML, CSV |

Design Review captures the model, checks, and evidence for the selected scope. Detailed Power Review adds rail budgets, contributors, source provenance, assumptions, and calculation steps. PDF and HTML include status colors; CSV and Markdown remain text-based registers where offered.

Choose a fresh filename for an export. Darah refuses to overwrite an existing export file. Review the generated document before sharing it, especially when the model still contains unknown values or manual checks.

## Save, reopen, and recover

Project files use `.darah.toml` and can be reopened through **File → Open**, the recent-project list, or a native/AppImage command-line argument.

Use **Save As** when making a separate design variant or retaining a copy for an older application build. If a project changes on disk after opening, Darah prevents a normal save from overwriting that external edit; save your work to a new filename and compare the files.

Undo and redo cover up to 64 in-session project edits. This history is not stored in the project file.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| F1 | Open Help. |
| Ctrl+K | Open commands and search for blocks or ports. |
| Ctrl+N / Ctrl+O | Create / open a project. |
| Ctrl+S / Ctrl+Shift+S | Save / Save As. |
| Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y | Undo / redo. |
| Ctrl+D | Duplicate selected blocks. |
| Delete | Delete the selected block, wire, or boundary. |
| Escape | Close Help or About; other behavior depends on the active editor. |

Text fields retain their usual editing shortcuts.

## Get help or request a feature

Use **Help → Help topics** for the built-in reference. **Help → About Darah EDA → Copy build info** provides the version and platform details needed for a useful issue report.

[Open an issue or feature request](https://github.com/citrineio/darah-eda/issues/new/choose) with the expected behavior, what happened, and the smallest reproducible example. Include the active operating mode and review scope for a calculation or review issue. Remove confidential design information before attaching a project or screenshot.
