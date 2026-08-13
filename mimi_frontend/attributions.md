# Attributions

## DINish Next

The self-hosted UI font is a locally retuned build of
[DINish](https://github.com/playbeing/dinish) by the DINish Project Authors,
licensed under the [SIL Open Font License 1.1](https://openfontlicense.org/).
Its source OpenType faces are kept in `next/`; the WOFF2 files under
`public/fonts/` are browser-compressed copies of the faces the UI uses.

Both sets carry one local patch. Every charstring in the upstream build stores
its advance as an **absolute** value, but the CFF Private DICT declared a
non-zero `nominalWidthX` (545–564, depending on the face), and a CFF renderer
*adds* `nominalWidthX` to whatever the charstring says. That put roughly 0.55em
of phantom tracking between every pair of letters on the rendered page. `hmtx`
was correct throughout, so anything reading advances from `hmtx` (fontTools,
most validators) saw a perfectly healthy font; only real renderers were wrong.

The patch sets `nominalWidthX` and `defaultWidthX` to 0 in all fifteen files, so
the charstring value is taken literally and agrees with `hmtx`. Outlines are
untouched: no charstring was recompiled. **Re-converting from a fresh upstream
download will reintroduce this**, so re-apply the patch if these files are ever
regenerated.

## Correct answer sound

correct by ertfelda -- https://freesound.org/s/243701/ -- License: Creative Commons 0

## Wrong answer sound

Error.wav by Autistic Lucario -- https://freesound.org/s/142608/ -- License: Attribution 4.0

## Triumphant success sound

Triumphant success by mokasza -- https://freesound.org/s/810330/ -- License: Attribution 4.0
