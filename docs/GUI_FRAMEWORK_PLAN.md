# GUI Integration Framework Plan

This document outlines the implementation plan for adding visual patching support to Quiver. The goal is to provide framework-agnostic primitives that enable GUI frontends (egui, iced, web canvas, etc.) to implement visual modular patching.

## Overview

**Current State**: Position serialization is complete (`ModuleDef.position`, `Patch::set_position/get_position`)

**Goal**: Provide a complete toolkit for building visual patch editors without dictating rendering technology

**Location**: New module `src/gui.rs` (feature-gated with `gui` feature)

---

## Phase 1: Core Geometry

Foundation types for spatial reasoning about patches.

### 1.1 Basic Types

```rust
/// 2D point
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// Rectangle (axis-aligned bounding box)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn contains(&self, point: Point) -> bool;
    pub fn intersects(&self, other: &Rect) -> bool;
    pub fn center(&self) -> Point;
    pub fn expand(&self, margin: f32) -> Rect;
}
```

### 1.2 Module Geometry

```rust
/// Standard module sizing based on port count
pub struct ModuleSizing {
    /// Base width for modules
    pub base_width: f32,
    /// Height per port row
    pub row_height: f32,
    /// Header height (for module name)
    pub header_height: f32,
    /// Port radius for hit detection
    pub port_radius: f32,
    /// Padding around ports
    pub port_padding: f32,
}

impl Default for ModuleSizing {
    fn default() -> Self {
        Self {
            base_width: 120.0,
            row_height: 24.0,
            header_height: 32.0,
            port_radius: 8.0,
            port_padding: 12.0,
        }
    }
}

/// Computed geometry for a module instance
pub struct ModuleGeometry {
    /// Bounding rectangle
    pub bounds: Rect,
    /// Input port positions (relative to bounds origin)
    pub input_ports: Vec<PortGeometry>,
    /// Output port positions (relative to bounds origin)
    pub output_ports: Vec<PortGeometry>,
}

pub struct PortGeometry {
    pub port_id: u32,
    pub name: String,
    pub kind: SignalKind,
    /// Center position relative to module origin
    pub position: Point,
    /// Hit detection radius
    pub radius: f32,
}
```

### 1.3 Geometry Calculator

```rust
/// Calculates module geometry from port specifications
pub struct GeometryCalculator {
    pub sizing: ModuleSizing,
}

impl GeometryCalculator {
    pub fn new(sizing: ModuleSizing) -> Self;

    /// Calculate geometry for a module at a given position
    pub fn calculate(&self, spec: &PortSpec, position: Point) -> ModuleGeometry;

    /// Calculate minimum bounds for a module
    pub fn minimum_bounds(&self, spec: &PortSpec) -> (f32, f32);
}
```

**Deliverables**:
- [ ] `Point`, `Rect` types with standard operations
- [ ] `ModuleSizing` configuration
- [ ] `ModuleGeometry`, `PortGeometry` types
- [ ] `GeometryCalculator` implementation
- [ ] Unit tests for geometry calculations

---

## Phase 2: Cable Routing

Visual cable path generation for connecting modules.

### 2.1 Cable Path

```rust
/// Bezier curve representation for a cable
pub struct CablePath {
    /// Start point (output port)
    pub start: Point,
    /// End point (input port)
    pub end: Point,
    /// Control points for cubic bezier
    pub control1: Point,
    pub control2: Point,
}

impl CablePath {
    /// Create a cable path between two points
    /// Uses horizontal flow (left-to-right) heuristics
    pub fn new(start: Point, end: Point) -> Self;

    /// Create with custom tension (0.0 = straight, 1.0 = very curved)
    pub fn with_tension(start: Point, end: Point, tension: f32) -> Self;

    /// Sample points along the curve for rendering
    pub fn sample(&self, segments: usize) -> Vec<Point>;

    /// Get point at parameter t (0.0 to 1.0)
    pub fn point_at(&self, t: f32) -> Point;

    /// Get tangent direction at parameter t
    pub fn tangent_at(&self, t: f32) -> Point;

    /// Find closest point on curve to a given point
    /// Returns (t, distance)
    pub fn closest_point(&self, point: Point) -> (f32, f32);

    /// Check if point is within distance of cable
    pub fn hit_test(&self, point: Point, threshold: f32) -> bool;
}
```

### 2.2 Cable Styles

```rust
/// Visual style for cables
#[derive(Debug, Clone)]
pub struct CableStyle {
    /// Base thickness
    pub thickness: f32,
    /// Whether to show signal flow animation direction
    pub animated: bool,
    /// Sag factor (gravity simulation)
    pub sag: f32,
}

/// Color scheme for signal types
pub struct SignalColors {
    pub audio: Color,
    pub cv_bipolar: Color,
    pub cv_unipolar: Color,
    pub volt_per_octave: Color,
    pub gate: Color,
    pub trigger: Color,
    pub clock: Color,
}

impl Default for SignalColors {
    fn default() -> Self {
        Self {
            audio: Color::rgb(0.91, 0.27, 0.38),        // #e94560
            cv_bipolar: Color::rgb(0.06, 0.20, 0.38),   // #0f3460
            cv_unipolar: Color::rgb(0.0, 0.71, 0.85),   // #00b4d8
            volt_per_octave: Color::rgb(0.56, 0.75, 0.43), // #90be6d
            gate: Color::rgb(0.98, 0.78, 0.31),         // #f9c74f
            trigger: Color::rgb(0.97, 0.59, 0.12),      // #f8961e
            clock: Color::rgb(0.62, 0.31, 0.87),        // #9d4edd
        }
    }
}
```

**Deliverables**:
- [ ] `CablePath` with cubic bezier implementation
- [ ] Point sampling for polyline rendering
- [ ] Hit testing for cable selection
- [ ] `CableStyle` and `SignalColors` defaults
- [ ] Unit tests for bezier math

---

## Phase 3: Hit Testing

Determine what UI element is at a given coordinate.

### 3.1 Hit Results

```rust
/// Result of a hit test query
#[derive(Debug, Clone, PartialEq)]
pub enum HitResult {
    /// Hit empty background
    Background,
    /// Hit a module body
    Module(NodeId),
    /// Hit a module's header/title bar (for dragging)
    ModuleHeader(NodeId),
    /// Hit an input port
    InputPort(NodeId, u32),
    /// Hit an output port
    OutputPort(NodeId, u32),
    /// Hit a cable (includes position along cable 0.0-1.0)
    Cable(CableId, f32),
}

/// Unique identifier for cables in the GUI layer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CableId(pub usize);
```

### 3.2 Hit Tester

```rust
/// Performs hit testing on a patch layout
pub struct HitTester {
    geometry_calc: GeometryCalculator,
    /// Cached module geometries
    module_geometries: HashMap<NodeId, ModuleGeometry>,
    /// Cached cable paths
    cable_paths: Vec<(CableId, CablePath, SignalKind)>,
}

impl HitTester {
    pub fn new(sizing: ModuleSizing) -> Self;

    /// Rebuild geometry cache from patch
    pub fn rebuild(&mut self, patch: &Patch);

    /// Update single module position (for drag operations)
    pub fn update_module_position(&mut self, node: NodeId, position: Point);

    /// Perform hit test at a point
    /// Returns hits in front-to-back order (cables on top)
    pub fn hit_test(&self, point: Point) -> Vec<HitResult>;

    /// Get the topmost hit at a point
    pub fn hit_test_top(&self, point: Point) -> HitResult;

    /// Find all modules in a selection rectangle
    pub fn modules_in_rect(&self, rect: Rect) -> Vec<NodeId>;

    /// Get geometry for a specific module
    pub fn module_geometry(&self, node: NodeId) -> Option<&ModuleGeometry>;

    /// Get all cable paths
    pub fn cable_paths(&self) -> &[(CableId, CablePath, SignalKind)];
}
```

**Deliverables**:
- [ ] `HitResult` enum with all interactive elements
- [ ] `HitTester` with geometry caching
- [ ] Rectangle selection for multi-select
- [ ] Efficient rebuilding on patch changes
- [ ] Unit tests for hit detection

---

## Phase 4: Introspection API

Runtime querying of module capabilities for dynamic UIs.

### 4.1 Parameter Introspection

```rust
/// Information about a controllable parameter
#[derive(Debug, Clone)]
pub struct ParamInfo {
    /// Parameter identifier
    pub id: String,
    /// Display name
    pub name: String,
    /// Current value
    pub value: f64,
    /// Minimum value
    pub min: f64,
    /// Maximum value
    pub max: f64,
    /// Default value
    pub default: f64,
    /// Value curve type
    pub curve: ParamCurve,
    /// Suggested UI control type
    pub control: ControlType,
    /// Unit label (Hz, ms, dB, etc.)
    pub unit: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamCurve {
    Linear,
    Exponential,
    Logarithmic,
    Stepped(u32), // Number of steps
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlType {
    Knob,
    Slider,
    Toggle,
    Dropdown,
    TextInput,
}
```

### 4.2 Module Introspection Trait

```rust
/// Extended introspection for GUI display
pub trait ModuleIntrospection: GraphModule {
    /// Get information about all tweakable parameters
    fn parameters(&self) -> Vec<ParamInfo> {
        Vec::new() // Default: no exposed parameters
    }

    /// Suggested minimum size for this module
    fn suggested_size(&self) -> Option<(f32, f32)> {
        None // Use default sizing
    }

    /// Custom port layout hints
    fn port_layout_hints(&self) -> PortLayoutHints {
        PortLayoutHints::default()
    }

    /// Category for module browser
    fn category(&self) -> &'static str {
        "Uncategorized"
    }

    /// Keywords for search
    fn keywords(&self) -> &[&'static str] {
        &[]
    }
}

#[derive(Debug, Clone, Default)]
pub struct PortLayoutHints {
    /// Group related ports together
    pub port_groups: Vec<PortGroup>,
    /// Ports that should be visually emphasized
    pub primary_ports: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct PortGroup {
    pub name: String,
    pub ports: Vec<u32>,
}
```

### 4.3 Module Browser Data

```rust
/// Data structure for module browser/palette
pub struct ModuleBrowserEntry {
    pub type_id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub keywords: Vec<String>,
    pub port_summary: PortSummary,
}

pub struct PortSummary {
    pub input_count: usize,
    pub output_count: usize,
    pub has_audio_in: bool,
    pub has_audio_out: bool,
    pub has_cv_in: bool,
    pub has_cv_out: bool,
}

impl ModuleRegistry {
    /// Get browser entries for all registered modules
    pub fn browser_entries(&self) -> Vec<ModuleBrowserEntry>;

    /// Search modules by query string
    pub fn search(&self, query: &str) -> Vec<ModuleBrowserEntry>;

    /// Get modules in a category
    pub fn by_category(&self, category: &str) -> Vec<ModuleBrowserEntry>;
}
```

**Deliverables**:
- [ ] `ParamInfo` and related types
- [ ] `ModuleIntrospection` trait
- [ ] Default implementations for built-in modules
- [ ] `ModuleBrowserEntry` for module palette
- [ ] Search functionality in `ModuleRegistry`

---

## Phase 5: Patch Operations & Undo/Redo

Command pattern for reversible patch editing.

### 5.1 Patch Commands

```rust
/// Reversible patch operation
#[derive(Debug, Clone)]
pub enum PatchCommand {
    /// Add a new module
    AddModule {
        name: String,
        module_type: String,
        position: Point,
    },

    /// Remove a module (stores full state for undo)
    RemoveModule {
        node_id: NodeId,
        /// Stored for undo
        module_def: ModuleDef,
        /// Cables that were connected (stored for undo)
        connected_cables: Vec<CableDef>,
    },

    /// Move module(s)
    MoveModules {
        moves: Vec<(NodeId, Point, Point)>, // (id, from, to)
    },

    /// Connect two ports
    Connect {
        from_node: NodeId,
        from_port: u32,
        to_node: NodeId,
        to_port: u32,
        attenuation: Option<f64>,
        offset: Option<f64>,
    },

    /// Disconnect a cable
    Disconnect {
        cable_id: CableId,
        /// Stored for undo
        cable_def: CableDef,
    },

    /// Change a parameter value
    SetParameter {
        node_id: NodeId,
        param_id: String,
        old_value: f64,
        new_value: f64,
    },

    /// Batch multiple commands (for undo as single operation)
    Batch {
        commands: Vec<PatchCommand>,
        description: String,
    },
}

impl PatchCommand {
    /// Get the inverse command (for undo)
    pub fn inverse(&self) -> PatchCommand;

    /// Human-readable description
    pub fn description(&self) -> String;
}
```

### 5.2 Patch History

```rust
/// Undo/redo history manager
pub struct PatchHistory {
    /// Commands that can be undone
    undo_stack: Vec<PatchCommand>,
    /// Commands that can be redone
    redo_stack: Vec<PatchCommand>,
    /// Maximum history size
    max_size: usize,
    /// Whether history is currently recording
    recording: bool,
}

impl PatchHistory {
    pub fn new(max_size: usize) -> Self;

    /// Execute a command and add to history
    pub fn execute(&mut self, patch: &mut Patch, command: PatchCommand) -> Result<(), PatchError>;

    /// Undo the last command
    pub fn undo(&mut self, patch: &mut Patch) -> Result<Option<String>, PatchError>;

    /// Redo the last undone command
    pub fn redo(&mut self, patch: &mut Patch) -> Result<Option<String>, PatchError>;

    /// Check if undo is available
    pub fn can_undo(&self) -> bool;

    /// Check if redo is available
    pub fn can_redo(&self) -> bool;

    /// Get description of next undo
    pub fn undo_description(&self) -> Option<&str>;

    /// Get description of next redo
    pub fn redo_description(&self) -> Option<&str>;

    /// Begin a batch operation (groups commands until end_batch)
    pub fn begin_batch(&mut self, description: &str);

    /// End batch operation
    pub fn end_batch(&mut self);

    /// Clear all history
    pub fn clear(&mut self);
}
```

### 5.3 Clipboard Operations

```rust
/// Clipboard for copy/paste operations
pub struct PatchClipboard {
    /// Copied module definitions
    modules: Vec<ModuleDef>,
    /// Copied internal cables (between copied modules)
    cables: Vec<CableDef>,
}

impl PatchClipboard {
    pub fn new() -> Self;

    /// Copy selected modules to clipboard
    pub fn copy(&mut self, patch: &Patch, selection: &[NodeId]);

    /// Cut selected modules (copy + delete)
    pub fn cut(&mut self, patch: &mut Patch, history: &mut PatchHistory, selection: &[NodeId]);

    /// Paste clipboard contents at position
    /// Returns handles to newly created modules
    pub fn paste(
        &self,
        patch: &mut Patch,
        history: &mut PatchHistory,
        position: Point,
        registry: &ModuleRegistry,
    ) -> Result<Vec<NodeId>, PatchError>;

    /// Check if clipboard has content
    pub fn has_content(&self) -> bool;

    /// Duplicate selection in place (offset by delta)
    pub fn duplicate(
        patch: &mut Patch,
        history: &mut PatchHistory,
        selection: &[NodeId],
        offset: Point,
        registry: &ModuleRegistry,
    ) -> Result<Vec<NodeId>, PatchError>;
}
```

**Deliverables**:
- [ ] `PatchCommand` enum with all operations
- [ ] Command execution and inversion
- [ ] `PatchHistory` with undo/redo stacks
- [ ] Batch command grouping
- [ ] `PatchClipboard` for copy/cut/paste
- [ ] Duplicate functionality
- [ ] Integration tests for command sequences

---

## Phase 6: Layout Algorithms

Automatic module arrangement.

### 6.1 Layout Trait

```rust
/// Auto-layout algorithm interface
pub trait AutoLayout {
    /// Compute positions for all modules in a patch
    fn layout(&self, patch: &Patch, bounds: Rect) -> HashMap<NodeId, Point>;
}
```

### 6.2 Hierarchical Layout

```rust
/// Signal-flow based layout (inputs left, outputs right)
pub struct HierarchicalLayout {
    /// Horizontal spacing between columns
    pub column_spacing: f32,
    /// Vertical spacing between modules in same column
    pub row_spacing: f32,
    /// Flow direction
    pub direction: FlowDirection,
}

#[derive(Debug, Clone, Copy)]
pub enum FlowDirection {
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
}

impl AutoLayout for HierarchicalLayout {
    fn layout(&self, patch: &Patch, bounds: Rect) -> HashMap<NodeId, Point> {
        // 1. Compute topological depth for each module
        // 2. Assign modules to columns by depth
        // 3. Order modules within columns to minimize cable crossings
        // 4. Compute final positions
    }
}
```

### 6.3 Force-Directed Layout

```rust
/// Physics-based layout using spring simulation
pub struct ForceDirectedLayout {
    /// Repulsion force between modules
    pub repulsion: f32,
    /// Spring constant for cables
    pub spring_constant: f32,
    /// Damping factor
    pub damping: f32,
    /// Maximum iterations
    pub max_iterations: usize,
    /// Convergence threshold
    pub threshold: f32,
}

impl AutoLayout for ForceDirectedLayout {
    fn layout(&self, patch: &Patch, bounds: Rect) -> HashMap<NodeId, Point> {
        // 1. Initialize positions (random or current)
        // 2. Iterate:
        //    - Calculate repulsion forces between all module pairs
        //    - Calculate spring forces along cables
        //    - Apply forces with damping
        //    - Check for convergence
        // 3. Return final positions
    }
}
```

### 6.4 Grid Snap

```rust
/// Snap positions to grid
pub struct GridSnap {
    pub grid_size: f32,
    pub enabled: bool,
}

impl GridSnap {
    pub fn snap(&self, point: Point) -> Point;
    pub fn snap_rect(&self, rect: Rect) -> Rect;
}
```

**Deliverables**:
- [ ] `AutoLayout` trait
- [ ] `HierarchicalLayout` implementation
- [ ] `ForceDirectedLayout` implementation
- [ ] `GridSnap` utility
- [ ] Layout quality metrics (cable crossings, etc.)

---

## Phase 7: Event Abstractions

Framework-agnostic input handling.

### 7.1 Input Events

```rust
/// Abstract input event (map from framework-specific events)
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Mouse/touch press
    PointerDown {
        position: Point,
        button: PointerButton,
        modifiers: Modifiers,
    },

    /// Mouse/touch release
    PointerUp {
        position: Point,
        button: PointerButton,
        modifiers: Modifiers,
    },

    /// Mouse/touch move
    PointerMove {
        position: Point,
        modifiers: Modifiers,
    },

    /// Scroll/zoom
    Scroll {
        position: Point,
        delta: Point,
        modifiers: Modifiers,
    },

    /// Key press
    KeyDown {
        key: Key,
        modifiers: Modifiers,
    },

    /// Key release
    KeyUp {
        key: Key,
        modifiers: Modifiers,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Primary,   // Left click
    Secondary, // Right click
    Middle,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool, // Cmd on Mac
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Delete,
    Backspace,
    Escape,
    Enter,
    Space,
    Tab,
    // Arrow keys
    Up, Down, Left, Right,
    // Letters (for shortcuts)
    A, C, D, V, X, Z,
    // Function keys
    F1, F2, // etc.
    // Other
    Other(char),
}
```

### 7.2 Interaction State Machine

```rust
/// Current interaction mode
#[derive(Debug, Clone)]
pub enum InteractionState {
    /// Idle, waiting for input
    Idle,

    /// Dragging module(s)
    DraggingModules {
        nodes: Vec<NodeId>,
        start_positions: Vec<Point>,
        current_offset: Point,
    },

    /// Drawing a new cable
    DraggingCable {
        from_node: NodeId,
        from_port: u32,
        is_output: bool,
        current_end: Point,
    },

    /// Rectangle selection
    RectangleSelect {
        start: Point,
        current: Point,
    },

    /// Panning the view
    Panning {
        start_pan: Point,
        start_pointer: Point,
    },

    /// Context menu open
    ContextMenu {
        position: Point,
        target: HitResult,
    },
}

/// Manages interaction state and processes events
pub struct InteractionManager {
    state: InteractionState,
    selection: HashSet<NodeId>,
    hit_tester: HitTester,
    history: PatchHistory,
    clipboard: PatchClipboard,
}

impl InteractionManager {
    /// Process an input event, returns actions to perform
    pub fn handle_event(
        &mut self,
        event: InputEvent,
        patch: &mut Patch,
    ) -> Vec<Action>;

    /// Get current selection
    pub fn selection(&self) -> &HashSet<NodeId>;

    /// Set selection
    pub fn set_selection(&mut self, nodes: impl IntoIterator<Item = NodeId>);

    /// Add to selection
    pub fn add_to_selection(&mut self, node: NodeId);

    /// Clear selection
    pub fn clear_selection(&mut self);

    /// Select all
    pub fn select_all(&mut self, patch: &Patch);
}

/// Actions that should be performed by the UI layer
#[derive(Debug, Clone)]
pub enum Action {
    /// Redraw needed
    Redraw,
    /// Show context menu
    ShowContextMenu { position: Point, items: Vec<MenuItem> },
    /// Hide context menu
    HideContextMenu,
    /// Module added (for announcements)
    ModuleAdded(NodeId),
    /// Selection changed
    SelectionChanged,
    /// Request text input (for renaming, etc.)
    RequestTextInput { initial: String, callback_id: u32 },
}
```

**Deliverables**:
- [ ] `InputEvent` abstraction
- [ ] `InteractionState` enum
- [ ] `InteractionManager` state machine
- [ ] Standard keyboard shortcuts (Ctrl+Z, Ctrl+C, etc.)
- [ ] Multi-select with Shift/Ctrl
- [ ] Integration tests for interaction sequences

---

## Phase 8: View Transform

Pan and zoom support.

### 8.1 View Transform

```rust
/// 2D view transformation (pan + zoom)
#[derive(Debug, Clone, Copy)]
pub struct ViewTransform {
    /// Pan offset in screen coordinates
    pub pan: Point,
    /// Zoom level (1.0 = 100%)
    pub zoom: f32,
}

impl ViewTransform {
    pub fn new() -> Self;

    /// Convert screen coordinates to world coordinates
    pub fn screen_to_world(&self, screen: Point) -> Point;

    /// Convert world coordinates to screen coordinates
    pub fn world_to_screen(&self, world: Point) -> Point;

    /// Get the visible world rect for a screen size
    pub fn visible_rect(&self, screen_size: (f32, f32)) -> Rect;

    /// Pan by a delta (in screen coordinates)
    pub fn pan_by(&mut self, delta: Point);

    /// Zoom to a level, keeping a point fixed
    pub fn zoom_to(&mut self, zoom: f32, fixed_point: Point);

    /// Zoom by a factor, keeping a point fixed
    pub fn zoom_by(&mut self, factor: f32, fixed_point: Point);

    /// Fit all content in view
    pub fn fit_content(&mut self, content_bounds: Rect, screen_size: (f32, f32), padding: f32);

    /// Reset to default view
    pub fn reset(&mut self);
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self {
            pan: Point { x: 0.0, y: 0.0 },
            zoom: 1.0,
        }
    }
}
```

### 8.2 Zoom Constraints

```rust
pub struct ZoomConstraints {
    pub min_zoom: f32,
    pub max_zoom: f32,
    pub zoom_step: f32, // For scroll wheel
}

impl Default for ZoomConstraints {
    fn default() -> Self {
        Self {
            min_zoom: 0.1,
            max_zoom: 4.0,
            zoom_step: 0.1,
        }
    }
}
```

**Deliverables**:
- [ ] `ViewTransform` with pan/zoom
- [ ] Coordinate conversion utilities
- [ ] Fit-to-content functionality
- [ ] Zoom constraints
- [ ] Smooth zoom animation support

---

## Implementation Order

| Phase | Name | Priority | Dependencies | Estimated Complexity |
|-------|------|----------|--------------|---------------------|
| 1 | Core Geometry | High | None | Low |
| 2 | Cable Routing | High | Phase 1 | Medium |
| 3 | Hit Testing | High | Phase 1, 2 | Medium |
| 4 | Introspection | Medium | None | Low |
| 5 | Undo/Redo | High | None | Medium |
| 6 | Layout Algorithms | Low | Phase 1 | High |
| 7 | Event Abstractions | Medium | Phase 3, 5 | High |
| 8 | View Transform | Medium | Phase 1 | Low |

**Recommended order**: 1 → 2 → 3 → 5 → 8 → 4 → 7 → 6

---

## File Structure

```
src/
├── gui/
│   ├── mod.rs           # Module exports, feature gate
│   ├── geometry.rs      # Phase 1: Point, Rect, ModuleGeometry
│   ├── cable.rs         # Phase 2: CablePath, CableStyle
│   ├── hit_test.rs      # Phase 3: HitTester, HitResult
│   ├── introspection.rs # Phase 4: ParamInfo, ModuleIntrospection
│   ├── commands.rs      # Phase 5: PatchCommand, PatchHistory
│   ├── clipboard.rs     # Phase 5: PatchClipboard
│   ├── layout.rs        # Phase 6: AutoLayout implementations
│   ├── events.rs        # Phase 7: InputEvent, InteractionManager
│   └── transform.rs     # Phase 8: ViewTransform
```

---

## Feature Flag

```toml
[features]
default = ["std"]
std = []
gui = ["std"]  # GUI support requires std
```

---

## Testing Strategy

1. **Unit tests** for all geometry calculations
2. **Property-based tests** for bezier math (using proptest)
3. **Integration tests** for command sequences
4. **Snapshot tests** for layout algorithms
5. **Example applications** demonstrating integration with:
   - egui (immediate mode)
   - iced (elm architecture)
   - Web canvas (via wasm-bindgen)

---

## Future Considerations

- **Minimap widget** - Overview of large patches
- **Module grouping** - Visual containers for related modules
- **Custom module skins** - Per-module visual customization
- **Accessibility** - Keyboard navigation, screen reader support
- **Touch support** - Multi-touch gestures for mobile/tablet
- **Collaboration** - Multi-user editing (requires networking)

---

## References

- [VCV Rack](https://vcvrack.com/) - Virtual modular synthesizer
- [Pure Data](https://puredata.info/) - Visual programming for audio
- [Max/MSP](https://cycling74.com/products/max) - Commercial visual patching
- [Blender Node Editor](https://docs.blender.org/manual/en/latest/interface/controls/nodes/) - General node graph UI
