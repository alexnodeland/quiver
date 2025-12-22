# Module Catalog

The module catalog provides a searchable registry of all available modules with metadata for building dynamic UIs.

## Browsing Modules

### Get All Modules

```typescript
const catalog = engine.catalog();

// Returns array of CatalogEntry:
// {
//   type_id: "Vco",
//   name: "VCO",
//   category: "oscillator",
//   description: "Voltage-controlled oscillator with multiple waveforms",
//   keywords: ["oscillator", "vco", "saw", "square", "triangle"],
//   inputs: [{ id: 0, name: "v_oct", kind: "VoltPerOctave" }, ...],
//   outputs: [{ id: 0, name: "out", kind: "Audio" }],
//   params: [{ id: "0", name: "frequency", ... }]
// }
```

### Filter by Category

```typescript
const oscillators = engine.by_category('oscillator');
const filters = engine.by_category('filter');
const utilities = engine.by_category('utility');
```

### Categories

| Category | Description | Examples |
|----------|-------------|----------|
| `oscillator` | Sound sources | Vco, Lfo, NoiseGenerator |
| `envelope` | Time-based modulation | AdsrEnvelope, SlewLimiter |
| `filter` | Frequency shaping | SvfFilter, DiodeLadderFilter |
| `amplifier` | Level control | Vca, Mixer, Attenuverter |
| `effect` | Audio effects | Saturator, Wavefolder, RingModulator |
| `utility` | CV/signal utilities | Quantizer, SampleAndHold, Clock |
| `io` | Input/output | ExternalInput, StereoOutput |

## Searching Modules

Full-text search with relevance scoring:

```typescript
const results = engine.search('filter');

// Returns matches sorted by relevance:
// [
//   { entry: {...}, score: 100 },  // Exact type_id match
//   { entry: {...}, score: 80 },   // Name contains "filter"
//   { entry: {...}, score: 50 },   // Keyword match
// ]
```

### Search Matching

| Match Type | Score | Example |
|------------|-------|---------|
| Exact type_id | 100 | "Vco" matches Vco |
| Name contains | 80-90 | "osc" matches "VCO" |
| Description | 60 | "waveform" matches VCO description |
| Keyword | 40-50 | "analog" matches tagged modules |
| Category | 10 | "filter" matches filter category |

## Module Metadata

Each catalog entry provides rich metadata for UI generation:

### Port Information

```typescript
const entry = catalog.find(m => m.type_id === 'SvfFilter');

// Input ports
entry.inputs.forEach(port => {
  console.log(port.id, port.name, port.kind);
  // 0, "input", "Audio"
  // 1, "cutoff", "CvBipolar"
  // 2, "resonance", "CvUnipolar"
});

// Output ports
entry.outputs.forEach(port => {
  console.log(port.id, port.name, port.kind);
  // 0, "lowpass", "Audio"
  // 1, "highpass", "Audio"
  // 2, "bandpass", "Audio"
});
```

### Parameter Information

```typescript
entry.params.forEach(param => {
  console.log(param);
  // {
  //   id: "0",
  //   name: "cutoff",
  //   min: 20.0,
  //   max: 20000.0,
  //   default: 1000.0,
  //   curve: "exponential",
  //   control_type: "knob",
  //   format: { type: "frequency" }
  // }
});
```

## Building a Module Browser UI

```tsx
function ModuleBrowser({ engine, onSelect }) {
  const [query, setQuery] = useState('');
  const [category, setCategory] = useState(null);

  const modules = useMemo(() => {
    if (query) {
      return engine.search(query).map(r => r.entry);
    }
    if (category) {
      return engine.by_category(category);
    }
    return engine.catalog();
  }, [query, category]);

  return (
    <div>
      <input
        value={query}
        onChange={e => setQuery(e.target.value)}
        placeholder="Search modules..."
      />

      <select onChange={e => setCategory(e.target.value || null)}>
        <option value="">All Categories</option>
        <option value="oscillator">Oscillators</option>
        <option value="filter">Filters</option>
        <option value="envelope">Envelopes</option>
        {/* ... */}
      </select>

      <ul>
        {modules.map(m => (
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

## Signal Type Colors

For cable visualization, use the standard signal colors:

```typescript
const colors = engine.signal_colors();
// {
//   audio: "#ff6b6b",      // Red
//   cv_bipolar: "#4ecdc4", // Cyan
//   cv_unipolar: "#95e879",// Green
//   volt_per_octave: "#ffd93d", // Yellow
//   gate: "#c44dff",       // Purple
//   trigger: "#ff6bcb",    // Magenta
//   clock: "#ffa94d"       // Orange
// }
```

## Port Compatibility

Check if two ports can be connected:

```typescript
const compat = engine.check_compatibility(
  'vco', 0,    // source: VCO output 0
  'vcf', 0     // dest: VCF input 0
);

// Returns: "exact" | "allowed" | "warning" | "incompatible"
```

| Result | Meaning | UI Hint |
|--------|---------|---------|
| `exact` | Same signal type | Green cable |
| `allowed` | Compatible types | Normal cable |
| `warning` | May clip or mismatch | Yellow cable |
| `incompatible` | Cannot connect | Prevent connection |
