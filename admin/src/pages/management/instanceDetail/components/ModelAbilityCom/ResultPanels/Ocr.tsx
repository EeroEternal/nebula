import { ReactMarkdown, IconButton } from '@/components';
import { copyToClipboard } from '@/utils';
import { Copy } from 'lucide-react';
import { Carousel, CarouselProps } from 'antd';
import { FC, useState } from 'react';

interface OcrProps {
  result?: {
    data: string[];
  };
}
const Ocr: FC<OcrProps> = ({ result }) => {
  const markdownList = result?.data || [];
  const [currentIndex, setCurrentIndex] = useState(0);

  const handleBeforeChange: CarouselProps['beforeChange'] = (_, next) => {
    setCurrentIndex(next);
  };
  return (
    <>
      <IconButton
        className="absolute top-3 right-3 !w-7 !h-7  hover:text-primary hover:bg-primary/15"
        onClick={() => copyToClipboard(markdownList[currentIndex] || '')}
      >
        <Copy size={16} />
      </IconButton>
      <div className="relative min-h-full">
        <Carousel
          arrows
          infinite={false}
          className="card-carousel !static pb-4"
          dots={false}
          beforeChange={handleBeforeChange}
        >
          {markdownList.map((item) => (
            <ReactMarkdown key={item} parseHtml>
              {item}
            </ReactMarkdown>
          ))}
        </Carousel>
      </div>
    </>
  );
};
export default Ocr;
