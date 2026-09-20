export const permissionsData = {
  models: '模型管理',
  instances: '实例管理',
  users: '用户管理',
  roles: '角色管理',
  secrets: '密钥管理',
  tasks: '微调任务',
  list: '列表',
  read: '查看',
  start: '后台运行实例',
  stop: '终止运行实例',
  register: '注册模型',
  unregister: '取消注册',
  add: '添加',
  delete: '删除',
  modify: '修改',
  cancel: '取消',
};

export const LetterReg = /[A-Za-z]/;

export const DigitsReg = /\d/;

export const PunctuationReg = /[`~!@#$%^&*()_+<>?:"{},.\/;'[\]]/;
/** 字母+数组+符号 */
export const PASSWORD_REGEX = /^(?=.*[a-zA-Z])(?=.*\d)(?=.*[`~!@#$%^&*()_+<>?:"{},.\/;'[\]])[A-Za-z\d`~!@#$%^&*()_+<>?:"{},.\/;'[\]]{8,30}$/;