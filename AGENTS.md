# FastDisco agent guide

- Keep FastDisco a small native Discord client: no browser engine, no
  telemetry, no hosted backend.
- The interface mirrors Discord's desktop layout. Keep controls where
  Discord puts them; a change to placement is a redesign.
- `src/ui/` draws views and pushes `Action`s; `App::apply` in `src/app.rs`
  runs them after the frame. Do not mutate state from inside a view.
- The interface is optimistic: a sent message shows immediately and the
  gateway confirms it later.
- Network work never blocks the UI thread.
- fastframe crates and the egui/winit forks move together, as in Spotifast
  and ZapFast.
- Never log tokens or credentials.
- Never use em dashes in prose.
