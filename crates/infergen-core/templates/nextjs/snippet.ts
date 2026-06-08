// After `infergen generate`, import your typed SDK:
import { track } from "./infergen.generated";

// Track a page view:
track.pageViewed({ page_path: window.location.pathname });

// Track signup:
track.userSignupCompleted({ method: "google" });

// Track a button click:
track.buttonClicked({ label: "Get Started" });
