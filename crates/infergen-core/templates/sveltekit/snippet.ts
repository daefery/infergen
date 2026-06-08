// After `infergen generate`, import your typed SDK:
import { track } from "$lib/infergen.generated";

// In +layout.ts or +page.ts:
export const load = ({ url }) => {
  track.pageViewed({ page_path: url.pathname });
};

// In a +page.server.ts action:
export const actions = {
  default: async ({ request }) => {
    track.formActionInvoked({ action_name: "default" });
  },
};
