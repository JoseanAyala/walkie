# UI polish

Making the pixel-OS look feel premium: craft within the style, not a new one.

- [ ] **One chrome.** Hide the native title bar (`titleBarStyle: Overlay`,
      `hiddenTitle`) so the dotted desk runs under the traffic lights; the
      `WALKIE.OS1` strip and the bare desk drag the window.
- [ ] **Depth everywhere.** Every `.win` gets the 4px hard shadow (not just the
      restart bar, Finish window and overlay pill).
- [ ] **Tactile buttons.** A 2px hard shadow that collapses on `:active` as the
      button shifts 2px down-right.
- [ ] **Stepped motion.** `steps()` animations: the main window opens on tab
      switch, `saved ✓` blinks in and fades, the restart bar slides up, the
      overlay pill enters and shows `✓ typed` before hiding, onboarding
      permissions flash when they turn OK. Respect reduced motion.
- [ ] **One type grid.** Line heights on the 11px grid (no 14px/16px strays in
      `.hint`, `.about`, onboarding, History, overlay label).
- [ ] **Less noise.** Drop the duplicated tab title; History actions on hover,
      rows grouped by day; About text wraps instead of hand-placed `<br>`s.
- [ ] **Pixel icons.** 11×11 icons for the Menu, permission rows, overlay chip;
      an illustration for the empty History.
- [ ] **Own the dropdowns.** A custom listbox instead of native `<select>`
      popups (microphone, language, model, injection).
- [ ] **Overlay meter.** Peak-hold on the VU meter; a fixed pill width so the
      label doesn't reflow between states.
- [ ] **Window limits.** `minWidth`/`minHeight` on settings so resizing can't
      break the grid.
