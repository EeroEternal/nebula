/* eslint-disable */

declare namespace API {
  type PageParams = {
    curPageNum?: number;
    numPerPage?: number;
  };
  type LoginParams = {
    username: string;
    password: string;
    token_expire_in_minutes?: number;
    autoLogin?: Boolean;
  };
  type LoginResult = {
    access_token: string;
    token_type: string;
    expire_in_minutes: number;
  };
  type roleItem = {
    role: string;
    update_ts: number;
  };
}
