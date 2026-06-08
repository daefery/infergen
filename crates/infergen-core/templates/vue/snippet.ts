// After `infergen generate`, import your typed SDK:
import { track } from "./infergen.generated";

// In a router guard or component:
router.afterEach((to) => {
  track.pageViewed({ route_name: to.name as string });
});

// In a form component:
function handleSubmit() {
  track.formSubmitted({ form_id: "signup" });
}
