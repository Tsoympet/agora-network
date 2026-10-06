/**
 * Forum storage for an operator. Posts are not consensus transactions.
 */

export const FORUM_TRUST =
  "Operator-hosted forum. Posts are not consensus transactions, not votes, and not treasury spends. The operator can omit, reorder, or alter posts.";

export type ForumReplyRecord = {
  id: string;
  authorAddress: string;
  authorUsername: string;
  body: string;
  source: string;
};

export type ForumRecord = {
  id: string;
  category: string;
  authorAddress: string;
  authorUsername: string;
  title: string;
  body: string;
  replies: ForumReplyRecord[];
  reportCount: number;
  source: string;
};

export function createForumServer(
  initial: readonly ForumRecord[] = [],
  options?: { onChange?: () => void },
) {
  const posts = initial.map((post) => ({
    ...post,
    replies: post.replies.map((reply) => ({ ...reply })),
  }));
  return {
    trust: FORUM_TRUST,
    list: () => posts,
    reply(input: { postId: string; reply: ForumReplyRecord }) {
      const post = posts.find((row) => row.id === input.postId);
      if (!post) throw new Error("unknown post");
      const body = input.reply.body.trim();
      if (!body) throw new Error("reply body required");
      if (!input.reply.authorAddress.trim()) throw new Error("reply author required");
      post.replies.push({ ...input.reply, body, source: "community submitted" });
      options?.onChange?.();
      return post.replies[post.replies.length - 1]!;
    },
  };
}

export type ForumServer = ReturnType<typeof createForumServer>;
