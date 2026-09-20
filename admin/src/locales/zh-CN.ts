import dashboard from './zh-CN/dashboard';
import global from './zh-CN/global';
import management from './zh-CN/management';
import menu from './zh-CN/menu';
import model from './zh-CN/model';
import models from './zh-CN/models';
import pages from './zh-CN/pages';
import app from './zh-CN/app';
import monitoring from './zh-CN/monitoring';
import tasks from './zh-CN/tasks';
import admin from './zh-CN/admin';

export default {
  ...pages,
  ...menu,
  ...app,
  ...dashboard,
  ...model,
  ...global,
  ...models,
  ...management,
  ...monitoring,
  ...tasks,
  ...admin,
};
