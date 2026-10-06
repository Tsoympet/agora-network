/** Operator search over community documents. It does not index chain state. */

export const SEARCH_TRUST =
  "Operator search index of community documents. Hits are not chain state and not a light-client proof. The operator chooses what is searchable.";

export type SearchDocument = {
  id: string;
  title: string;
  body: string;
};

export function createSearchIndex(documents: readonly SearchDocument[]) {
  const rows = documents.map((doc) => ({ ...doc }));
  return {
    trust: SEARCH_TRUST,
    query(q: string) {
      const needle = q.trim().toLowerCase();
      if (!needle) return [];
      return rows.filter(
        (doc) => doc.title.toLowerCase().includes(needle) || doc.body.toLowerCase().includes(needle),
      );
    },
  };
}
