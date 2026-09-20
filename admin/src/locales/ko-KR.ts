import dashboard from './ko-KR/dashboard';
import global from './ko-KR/global';
import management from './ko-KR/management';
import menu from './ko-KR/menu';
import model from './ko-KR/model';
import pages from './ko-KR/pages';
import app from './ko-KR/app';
import monitoring from './ko-KR/monitoring';
import models from './ko-KR/models';
import tasks from './ko-KR/tasks';
import admin from './ko-KR/admin';
export default {
  ...menu,
  ...app,
  ...pages,
  ...dashboard,
  ...global,
  ...model,
  ...management,
  ...monitoring,
  ...models,
  ...tasks,
  ...admin,
};
