# v0.2 editor behavior

The accepted visual reference is [ui-v0.2-reference.png](ui-v0.2-reference.png). The Windows application uses a monochrome gray/white palette, static internal background, translucent surfaces, 14 px backdrop blur, system fonts and neutral line icons. It does not sample the desktop background.

The custom titlebar holds presets, compact routing, transport, settings and native window controls. The filter library stays above a single horizontal chain. A fixed inspector below the chain edits the selected filter through the existing shared numeric metadata. Audio routing, monitoring and buffer preferences move into a settings sheet. Existing session schema and native DSP commands remain unchanged.

Pointer dragging starts after 6 px, lifts the card gently at 1.02 scale, previews neighboring displacement and drops over 180 ms. The controlled preview is separate from the session: only a valid release commits an insertion/reorder. Escape or release outside the chain cancels. Automatic edge scrolling and wheel-to-horizontal scrolling are local to the chain. Reduced-motion preference removes the lift/drop transition.

Dragging uses the card's title/blank button area; bypass and menu buttons remain separate. The menu provides previous/next, duplicate and delete. Alt+left/right reorders the selected card, with Alt+up/down aliases retained. Space/Enter perform ordinary button activation rather than starting a separate keyboard drag mode. Clicking an insertion plus and then a library filter is the alternative to drag insertion. Deleting the selected card selects its neighbor; an empty chain shows an empty inspector.

The default window is 1100×760, with a 900×640 minimum. Five cards fit at the default width; longer chains scroll. Compared with the concept image, the scrollbar appears only when content overflows, and the gain card uses its actual 0 dB default. The titlebar uses real device names and the UI displays real save/error/audio status. Glass effects use CSS rather than a raster mockup; the reference image is not shipped as a runtime background.

New dependencies are the matching @dnd-kit/react and @dnd-kit/dom packages. React status polling does not rerender the memoized chain editor when its props are unchanged. Pointer events batch geometry reads and update preview state only when insertion position changes. No extra animation library is used.
