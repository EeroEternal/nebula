import { ReactMarkdown } from '@/components';
import { copyToClipboard } from '@/utils';
import { CopyOutlined } from '@ant-design/icons';
import { Carousel, CarouselProps } from 'antd';
import { FC, useState } from 'react';
import styles from './styles.less';

interface ResultCarouselMdProps {
  results?: string[];
}
const ResultCarouselMd: FC<ResultCarouselMdProps> = ({ results }) => {
  const markdownList = results || [];
  const [currentIndex, setCurrentIndex] = useState(0);

  const handleBeforeChange: CarouselProps['beforeChange'] = (_, next) => {
    setCurrentIndex(next);
  };
  return (
    <div className="flex flex-col justify-between gap-y-[10px] h-[calc(100%-32px)] relative">
      <Carousel
        arrows
        infinite={false}
        rootClassName={styles['result-carousel-md']}
        dots={false}
        beforeChange={handleBeforeChange}
      >
        {markdownList.map((item) => (
          <ReactMarkdown key={item} parseHtml>
            {item}
          </ReactMarkdown>
        ))}
      </Carousel>
      <CopyOutlined
        className="absolute top-[-48px] right-0 cursor-pointer hover:text-primary"
        onClick={() => copyToClipboard(markdownList[currentIndex] || '')}
      />
    </div>
  );
};
export default ResultCarouselMd;
