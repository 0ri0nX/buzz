import { useAgentManagement } from "@/features/agents/useAgentManagement";
import { RowviaManagementDialog } from "@/features/agents/rowvia-management/RowviaManagementDialog";
import { ProjectChannelRequestDialog } from "@/features/projects/ui/ProjectChannelRequestDialog";
import { AgentCardDialogs } from "./AgentCardViewerDialog";
import { AgentDialog } from "./AgentDialog";
import { useSavedAgentRegistration } from "../useSavedAgentRegistration";
import { SavedAgentRegistrationReviewDialog } from "./SavedAgentRegistrationReviewDialog";

/** Global review surfaces opened by owned agents through the Buzz harness. */
export function AgentManagementDialogs() {
  const management = useAgentManagement();
  const registration = useSavedAgentRegistration();

  return (
    <>
      {management.request?.action === "create" ? (
        <AgentDialog
          definitionError={
            management.error ? new Error(management.error) : null
          }
          initialValues={management.createInitialValues}
          isDefinitionPending={
            management.isPending || registration.state?.phase === "loading"
          }
          mode="definition"
          onOpenChange={(open) => {
            if (!open) management.dismiss();
          }}
          onSubmitDefinition={management.submitCreate}
          onSaveCodex={(name, prompt) =>
            management.submitSavedCodex(name, prompt, registration.save)
          }
          runtimes={management.runtimes}
          runtimeCatalogStatus={management.runtimeCatalogStatus}
        />
      ) : null}
      {management.request?.action === "update" ? (
        <AgentDialog
          description=""
          error={management.editError ? new Error(management.editError) : null}
          initialValues={management.editInitialValues}
          isPending={management.isPending}
          mode="definition-edit"
          onOpenChange={(open) => {
            if (!open) management.dismiss();
          }}
          onSubmit={management.submitUpdate}
          open
          runtimes={management.runtimes}
          runtimeCatalogStatus={management.runtimeCatalogStatus}
          submitLabel="Save changes"
          title="Edit agent"
        />
      ) : null}
      <ProjectChannelRequestDialog />
      <SavedAgentRegistrationReviewDialog registration={registration} />
      <RowviaManagementDialog />
      <AgentCardDialogs />
    </>
  );
}
