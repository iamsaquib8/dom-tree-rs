// Type of the value you get from `JSON.parse(parse(html))`.
//
// `parse()` (and `query()`) return JSON *strings*; parse them in JS and they
// match these types.

export type DomNode =
  | {
      type: "element";
      tag: string;
      attrs: Record<string, string>;
      children: DomNode[];
    }
  | { type: "text"; value: string }
  | { type: "comment"; value: string }
  | { type: "doctype"; value: string };

export interface DomDocument {
  type: "document";
  /** Number of recoverable problems encountered while parsing (0 = clean). */
  errors: number;
  children: DomNode[];
}
