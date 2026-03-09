export type TimelineMessage = {
  id: string;
  author: string;
  role: string;
  time: string;
  body: string;
  accent?: boolean;
  pending?: boolean;
};
