/** One help topic: a term (button, gesture, setting) and what it does. */
export interface HelpItem {
  term: string;
  text: string;
}

export interface HelpSection {
  id: string;
  title: string;
  intro?: string;
  items: HelpItem[];
}
