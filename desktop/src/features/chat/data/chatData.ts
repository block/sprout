export type Message = {
  author: string;
  role: string;
  time: string;
  body: string;
  accent?: boolean;
};

export const currentChannel = {
  name: "product-launch",
  description: "Static channel layout with a simple chat pane.",
};

export const messages: Message[] = [
  {
    author: "Maya",
    role: "Product",
    time: "9:12 AM",
    body: "Let’s keep this first pass focused on layout: sidebar, message area, and composer.",
  },
  {
    author: "Elena",
    role: "Engineering",
    time: "9:24 AM",
    body: "Agreed. The content can stay minimal until we wire up real channels and messages.",
    accent: true,
  },
];
