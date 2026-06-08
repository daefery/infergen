// After `infergen generate`, import your typed SDK:
import { track } from "./infergen.generated";

app.use((req, res, next) => {
  track.apiRequestReceived({ method: req.method, path: req.path });
  next();
});

app.post("/login", (req, res) => {
  // after auth attempt:
  track.userLoginAttempted({ success: true });
  res.json({ ok: true });
});
