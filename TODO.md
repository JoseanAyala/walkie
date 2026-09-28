# UI polish

Making the pixel-OS look feel premium: craft within the style, not a new one.

- [x] **One chrome.** Hide the native title bar (`titleBarStyle: Overlay`,
      `hiddenTitle`) so the dotted desk runs under the traffic lights; the
      `WALKIE.OS1` strip and the bare desk drag the window.
- [x] **Depth everywhere.** Every `.win` gets the 4px hard shadow (not just the
      restart bar, Finish window and overlay pill).
- [x] **Tactile buttons.** A 2px hard shadow that collapses on `:active` as the
      button shifts 2px down-right.
- [x] **Stepped motion.** `steps()` animations: the main window opens on tab
      switch, `saved ✓` blinks in and fades, the restart bar slides up, the
      overlay pill enters and shows `✓ typed` before hiding, onboarding
      permissions flash when they turn OK. Respect reduced motion.
- [x] **One type grid.** Line heights on the 11px grid (no 14px/16px strays in
      `.hint`, `.about`, onboarding, History, overlay label).
- [x] **Less noise.** Drop the duplicated tab title; History actions on hover,
      rows grouped by day; About text wraps instead of hand-placed `<br>`s.
- [x] **Pixel icons.** 11×11 icons for the Menu, permission rows, overlay chip;
      an illustration for the empty History.
- [x] **Own the dropdowns.** A custom listbox instead of native `<select>`
      popups (microphone, language, model, injection).
- [x] **Window limits.** `minWidth`/`minHeight` on settings so resizing can't
      break the grid.
- [x] **Overlay meter.** A scrolling wave (dB-scaled), a "can't hear you" hint,
      and a layout that holds still from REC to DONE.
