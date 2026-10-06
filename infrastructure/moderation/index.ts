/** Operator moderation queue. A report does not change reputation or consensus. */

export const MODERATION_TRUST =
  "Operator moderation queue. A report is not a reputation event and not a consensus transaction. The operator can ignore it.";

export function createModerationPipeline() {
  const reports: { postId: string; reason: string }[] = [];
  return {
    trust: MODERATION_TRUST,
    report(input: { postId: string; reason: string }) {
      if (!input.postId.trim() || !input.reason.trim()) {
        throw new Error("postId and reason required");
      }
      reports.push({ postId: input.postId, reason: input.reason });
      return { accepted: true, count: reports.length };
    },
    list: () => reports,
  };
}
