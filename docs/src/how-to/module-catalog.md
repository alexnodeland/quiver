# Module Catalog

The WASM `QuiverEngine` exposes a searchable catalog of every registered module, with
metadata for building dynamic "add module" UIs. All methods are on the engine returned
by `createEngine()` (see [Browser & App Integration](./browser-integration.md)).

> Module identifiers are lowercase `snake_case` — `"vco"`, `"svf"`, `"adsr"`,
> `"delay_line"`, `"scale_quantizer"` — matching each module's Rust `type_id()`.

## Browsing Modules

### Get the Full Catalog

```typescript
const catalog = engine.get_catalog();
// CatalogResponse:
// {
//   modules: ModuleCatalogEntry[],
//   categories: string[],   // unique, sorted
// }
```

Each `ModuleCatalogEntry` looks like:

```typescript
// {
//   type_id: "vco",
//   name: "VCO",
//   category: "Oscillators",
//   description: "Multi-waveform voltage-controlled oscillator",
//   keywords: ["oscillator", "vco", "saw", "square", "triangle"],
//   ports: { inputs: 5, outputs: 4, has_audio_in: false, has_audio_out: true },
//   tags: ["essential", "analog"]
// }
```

The catalog entry carries a **port count summary** (`ports`), not the full port list.
Fetch detailed ports for a type with `get_port_spec` (below).

### List Categories

```typescript
const categories = engine.get_categories(); // string[], e.g. ["Oscillators", "Filters", ...]
```

### Filter by Category

```typescript
const oscillators = engine.get_modules_by_category('Oscillators');
const filters = engine.get_modules_by_category('Filters');
```

## Searching Modules

Full-text search returns matching entries ranked by relevance:

```typescript
const results = engine.search_modules('filter');
// ModuleCatalogEntry[], best matches first — e.g. svf, diode_ladder, parametric_eq
```

Matching considers the `type_id`, `name`, `description`, `keywords`, and `category`, so
queries like `"acid"`, `"reverb"`, or `"pitch"` all work.

## Detailed Port Information

For the concrete input/output ports of a module type, call `get_port_spec` with its
`type_id`. This returns the module's `PortSpec` (`{ inputs, outputs }`), where each port
has `id`, `name`, and `kind`:

```typescript
const spec = engine.get_port_spec('svf');
// {
//   inputs: [
//     { id: 0, name: "in",     kind: "Audio" },
//     { id: 1, name: "cutoff", kind: "CvUnipolar" },
//     { id: 2, name: "res",    kind: "CvUnipolar" },
//     ...
//   ],
//   outputs: [
//     { id: 10, name: "lp", kind: "Audio" },
//     { id: 11, name: "bp", kind: "Audio" },
//     { id: 12, name: "hp", kind: "Audio" },
//     { id: 13, name: "notch", kind: "Audio" },
//   ],
// }
```

Port `kind` is one of the [signal types](../appendix/signal-cheatsheet.md): `Audio`,
`CvBipolar`, `CvUnipolar`, `VoltPerOctave`, `Gate`, `Trigger`, `Clock`.

## Signal Colors

For cable visualization, the engine provides the default signal-type palette:

```typescript
const colors = engine.get_signal_colors();
// {
//   audio: "#e94560",           // red
//   cv_bipolar: "#0f3460",      // dark blue
//   cv_unipolar: "#00b4d8",     // cyan
//   volt_per_octave: "#90be6d", // green
//   gate: "#f9c74f",            // yellow
//   trigger: "#f8961e",         // orange
//   clock: "#9d4edd",           // purple
// }
```

## Port Compatibility

Check whether two **signal kinds** can be connected. Pass the `kind` strings (as found
on a port spec), not port references:

```typescript
const compat = engine.check_compatibility('CvBipolar', 'Audio');
// { status: "allowed" }
// or { status: "exact" }
// or { status: "warning", message: "..." }
```

| `status` | Meaning | UI hint |
|----------|---------|---------|
| `exact` | Identical signal type | Green cable |
| `allowed` | Different but valid | Normal cable |
| `warning` | Works, but may clip or mismatch | Yellow cable + tooltip |

## Building a Module Browser UI

```tsx
function ModuleBrowser({ engine, onSelect }) {
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState<string | null>(null);

  const modules = useMemo(() => {
    if (query) return engine.search_modules(query);
    if (category) return engine.get_modules_by_category(category);
    return engine.get_catalog().modules;
  }, [engine, query, category]);

  const categories = useMemo(() => engine.get_categories(), [engine]);

  return (
    <div>
      <input
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Search modules..."
      />

      <select onChange={(e) => setCategory(e.target.value || null)}>
        <option value="">All Categories</option>
        {categories.map((c) => (
          <option key={c} value={c}>{c}</option>
        ))}
      </select>

      <ul>
        {modules.map((m) => (
          <li key={m.type_id} onClick={() => onSelect(m.type_id)}>
            <strong>{m.name}</strong>
            <span>{m.category}</span>
            <p>{m.description}</p>
          </li>
        ))}
      </ul>
    </div>
  );
}
```

Add a chosen module with `engine.add_module(type_id, name)` — for example
`engine.add_module('vco', 'osc1')`.
