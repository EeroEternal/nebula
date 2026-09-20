import dashboard from './en-US/dashboard';
import global from './en-US/global';
import management from './en-US/management';
import menu from './en-US/menu';
import model from './en-US/model';
import pages from './en-US/pages';
import app from './en-US/app';
import monitoring from './en-US/monitoring';
import models from './en-US/models';
import tasks from './en-US/tasks';
import admin from './en-US/admin';

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
