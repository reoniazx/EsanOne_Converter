# Right-Click Output Naming Design

## Goal

When an audio file is opened through EsanOne Convert's Windows context menu, write each generated stem next to the source file and give it a descriptive prefix. Add a context-menu action that analyzes Key/BPM and creates a renamed copy next to the source file.

## Approved behavior

- Separation output is flattened into the source file's parent directory.
- Stem names use prefixes: `[Vocal]`, `[Drum]`, `[Bass]`, `[Other]`, `[Guitar]`, `[Piano]`, and `[INST]` where the backend returns an instrumental mix.
- The remainder of each stem filename is the original source filename stem, preserving the source extension.
- Existing files are never overwritten. A numeric suffix is added before the extension when needed, for example `[Vocal] song (1).wav`.
- The Windows context menu includes `Analyze Key & BPM`.
- Key/BPM analysis shows an in-app Toast such as `Key: Am · BPM: 120`. It does not create or modify any audio file.
- Existing app-internal conversions continue to use the current app-data `exports` directory.

## Architecture

The startup CLI carries an explicit output mode for context-menu separation and analysis. The separation worker continues to let each backend write into its normal model/input subdirectory, then flattens and renames the completed files into the requested output directory. A shared filename allocator prevents collisions. Key/BPM is a CLI operation that runs the existing analyzer and sends its result to the frontend, where a bilingual Toast is shown.

## Error handling

- Missing input files, unsupported models, analyzer failures, and output-directory failures return a clear error.
- If one stem cannot be moved, the operation reports failure; already-created outputs are left visible for diagnosis and are never silently overwritten.
- Normal UI analysis remains display-only; only the explicit context-menu analysis command creates a renamed copy.

## Testing

- Unit tests cover filename prefix mapping, flattening, collision suffixes, CLI parsing, and Key/BPM Toast formatting.
- Rust tests cover context-menu command registration; no audio copy helper is needed for Key/BPM.
- Existing frontend, Rust, format, and production build checks remain required.
