export type TableListItem = {
  name: string;
  secrets: srting;
  created_ts: number;
};

export type DeleteParams = {
  name: string;
  secrets: string;
}

export type CreateParams = {
  name: string;
}