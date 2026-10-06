/**
 * Forum storage for an operator. Posts are not consensus transactions.
 */

export const FORUM_TRUST =
  "Operator-hosted forum. Posts are not consensus transactions, not votes, and not treasury spends. The operator can omit, reorder, or alter posts.";

export type ForumRecord = {
  id: string;
  category: string;
  authorAddress: string;
  authorUsername: string;
  title: string;
  body: string;
  replies: unknown[];
  reportCount: number;
  source: string;
};

export function createForumServer(initial: readonly ForumRecord[] = []) {
  const posts = initial.map((post) => ({ ...post, replies: [...post.replies] }));
  return {
    trust: FORUM_TRUST,
    list: () => posts,
  };
}

export type ForumServer = ReturnType<typeof createForumServer>;
