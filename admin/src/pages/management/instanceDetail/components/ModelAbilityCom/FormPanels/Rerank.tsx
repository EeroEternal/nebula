import { ProFormTextArea, ProFormList, ProFormText, ModalForm } from '@ant-design/pro-components';
import type { FormListActionType, ModalFormProps } from '@ant-design/pro-components';
import { FC, useRef } from 'react';
import { Plus, FileBraces, Trash2 } from 'lucide-react';
import { l, lGet } from '@/utils/intl';
import { isJSON } from '@/utils';
import type { FormPanelProps } from '../modelAbilityConfig';
import { IconButton } from '@/components';

const documentsExample = JSON.stringify(
  ['制定学习计划可以提高效率', '学习时保持专注很重要', '多做运动有助于提高记忆力'],
  null,
  2,
);

const iconBtnClass =
  '!w-7 !h-7 bg-white border border-[color:var(--c-border-light)] text-muted hover:text-primary hover:bg-white';

const Rerank: FC<FormPanelProps> = ({ form }) => {
  const documentsRef = useRef<FormListActionType>();
  const onFinish: ModalFormProps['onFinish'] = (values) => {
    const inputText = values.copyText.trim().replace(/'([^']*)'/g, '"$1"');
    try {
      const value = JSON.parse(inputText);
      form.setFieldsValue({
        documents: value.map((item: string) => ({ corpus: item })),
      });
    } catch (error) {}
    return Promise.resolve(true);
  };
  return (
    <>
      <ProFormTextArea
        rules={[{ required: true }]}
        name="query"
        placeholder={l('models.instances.detail.promptRerank.placeholder')}
        fieldProps={{
          rows: 4,
        }}
      />
      <div className="ml-1 mb-2.5 w-full flex justify-between items-center">
        <span className="font-medium">{l('models.instances.detail.documents')}</span>
        <div className="flex items-center gap-1.5">
          <ModalForm
            title="JSON"
            trigger={
              <IconButton className={iconBtnClass}>
                <FileBraces size={16} />
              </IconButton>
            }
            modalProps={{
              destroyOnClose: true,
            }}
            onFinish={onFinish}
          >
            <ProFormTextArea
              name="copyText"
              placeholder={documentsExample}
              fieldProps={{
                rows: 6,
              }}
              rules={[
                {
                  required: true,
                  validator: async (_rule: unknown, value: string) => {
                    if (isJSON(value.trim().replace(/'([^']*)'/g, '"$1"'))) {
                      return;
                    }
                    throw new Error(lGet('models.instances.detail.documents.rules') as string);
                  },
                },
              ]}
            />
          </ModalForm>
          <IconButton
            className={iconBtnClass}
            onClick={() => documentsRef.current?.add?.({})}
            aria-label={l('global.actions.add')}
          >
            <Plus size={16} />
          </IconButton>
        </div>
      </div>

      <ProFormList
        initialValue={[{}]}
        actionRef={documentsRef}
        name="documents"
        copyIconProps={false}
        creatorButtonProps={false}
        min={1}
        className="!mb-0"
        actionRender={(field) => {
          return [
            <IconButton
              key="delete"
              onClick={() => documentsRef.current?.remove(field.name)}
              className="-mt-1 !w-6 !h-6 rounded-md group"
            >
              <Trash2 size={16} className="group-hover:text-danger text-muted" />
            </IconButton>,
          ];
        }}
      >
        <ProFormText
          name="corpus"
          rules={[{ required: true }]}
          placeholder={l('models.instances.detail.documents.item')}
          colProps={{ span: 24 }}
        />
      </ProFormList>
    </>
  );
};
export default Rerank;
