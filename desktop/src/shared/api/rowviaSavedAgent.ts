import { invokeTauri } from "./tauri";
import { createSavedAgentOwnerApi } from "./rowviaSavedAgentContract";
export type {
  SavedAgentReview,
  SavedAgentAuthorization,
  SavedAgentPending,
  SavedAgentApproval,
  SavedCodexInput,
} from "./rowviaSavedAgentContract";

// Existing central Tauri transport, not a new client/provider or URL selector.
const owner = createSavedAgentOwnerApi((command, args) =>
  invokeTauri<unknown>(command, args),
);
export const reviewSavedAgentPreparation = owner.review;
export const authorizeSavedAgentRegistration = owner.authorize;
export const saveCodexAgent = owner.save;
export const listSavedAgentPending = owner.pending;
export const approveSavedAgentRegistration = owner.approve;
