export type TableListItem = {
  account: string;
  username: string;
  role: string;
  email: string;
  status: string;
  last_login_ts: number;
};

export type CreateUserParams = {
  username: string;
  email: string;
  password: string;
  role: string;
  duplicatePassword?: string;
}

export type UpdateUserParams = {
  account: string;
  username: string;
  status: string;
  role: string;
  duplicatePassword?: string;
}