import { App } from 'antd';
import { useEffect } from 'react';
import { history, useParams } from '@umijs/max';
import { PageContainer } from '@/components';
import { l, lGet } from '@/utils/intl';
import NewTaskForm from '../components/NewTaskForm';

const CreateFinetune = () => {
  const { message } = App.useApp();
  const params = useParams();
  const submitCallBack = () => {
    message.success(lGet('global.message.editSuccess'));
    history.push('/tasks/finetune');
  };
  useEffect(() => {
    if (!params.id) {
      history.push('/tasks/finetune');
    }
  }, [params]);
  return (
    <PageContainer showBreadcrumb title={l('menu.tasks.finetune.edit')}>
      <NewTaskForm taskId={params.id} onSubmitCallback={submitCallBack} />
    </PageContainer>
  );
};
export default CreateFinetune;
