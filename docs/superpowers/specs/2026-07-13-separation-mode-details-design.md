# Separation Mode Details Design

## Goal

Make the three separation choices self-explanatory by showing the real model name in parentheses and the outputs, benefits, and limitations of the selected mode.

## Mode copy

- Vocals (MelBandRoFormer): Vocals and instrumental. Best vocal isolation for karaoke and remixing; does not split individual instruments.
- Instruments 4 (HT-Demucs): Vocals, drums, bass, and other. Balanced four-stem separation for general mixing; vocal and instrument bleed may remain.
- Instruments 6 (HT-Demucs 6s): Vocals, drums, bass, other, guitar, and piano. Adds guitar and piano control; slower and uses more GPU/CPU and disk space.

## UI behavior

- Each mode card uses the model name in parentheses.
- A detail panel below the cards updates with the selected mode and presents “Separates”, “Advantages”, and “Limitations”.
- The panel uses the existing Thai-first i18n system and collapses to one column on narrow windows.

## Verification

- i18n tests cover the new Thai labels and English fallback behavior.
- Frontend build and the complete Vitest suite must pass.
