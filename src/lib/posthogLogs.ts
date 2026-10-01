import { PUBLIC_POSTHOG_HOST, PUBLIC_POSTHOG_PROJECT_TOKEN } from "$env/static/public";
import posthog from "posthog-js";

function canExportLogs(): boolean {
  return Boolean(PUBLIC_POSTHOG_PROJECT_TOKEN && PUBLIC_POSTHOG_HOST);
}

// This logger intentionally exports only the explicit lifecycle records below.
export const posthogLogs = {
  applicationBooted(): void {
    if (canExportLogs()) {
      posthog.logger.info("application_booted", { component: "desktop_client" });
    }
  },

  onboardingCompleted(): void {
    if (canExportLogs()) {
      posthog.logger.info("onboarding_completed", { component: "onboarding" });
    }
  },

  onboardingSkipped(): void {
    if (canExportLogs()) {
      posthog.logger.info("onboarding_skipped", { component: "onboarding" });
    }
  },

  fleetShown(detectedClis: number): void {
    if (canExportLogs()) {
      posthog.logger.info("fleet_shown", {
        component: "onboarding",
        detectedClis,
      });
    }
  },

  fleetLaunched(cliCount: number): void {
    if (canExportLogs()) {
      posthog.logger.info("fleet_launched", {
        component: "onboarding",
        cliCount,
      });
    }
  },

  fleetFirstCompleted(seconds: number): void {
    if (canExportLogs()) {
      posthog.logger.info("fleet_first_completed", {
        component: "onboarding",
        seconds,
      });
    }
  },
};
