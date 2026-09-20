import { App } from 'antd';
import { history } from '@umijs/max';
import { PageContainer } from '@/components';
import { l, lGet } from '@/utils/intl';
import NewTaskForm from '../components/NewTaskForm';

const CreateFinetune = () => {
  const { message } = App.useApp();
  const submitCallBack = () => {
    message.success(lGet('global.message.addSuccess'));
    history.push('/tasks/finetune');
  };

  return (
    <PageContainer showBreadcrumb title={l('menu.tasks.finetune.create')}>
      <NewTaskForm onSubmitCallback={submitCallBack} />
    </PageContainer>
  );
};
export default CreateFinetune;
