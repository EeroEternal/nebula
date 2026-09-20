/**
 * loading 占位
 * 解决首次加载时白屏的问题
 */
 (function () {
  const _root = document.querySelector('#root');
  if (_root && _root.innerHTML === '') {
    _root.innerHTML = `
      <style>
        html, body, #root {
          height: 100%;
          margin: 0;
          padding: 0;
        }
        .ant-spin-spinning-wrap {
          display: flex;
          height: 100vh;
          justify-content: center;
          align-items: center;
          background-color: #fff;
        }
        .ant-spin-spinning {
          position: relative;
          color: #0045B0;
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
        }

        .ant-spin-dot-holder {
          display: inline-block;
          font-size: 20px;
          width: 
        }

        .ant-spin-dot-spin {
          position: relative;
          display: inline-block;
          width: 20px;
          height: 20px;
          animation: antRotate 1.2s infinite linear;
          transform: rotate(45deg);
        }

        .ant-spin-dot-item {
          position: absolute;
          display: block;
          width: 9px;
          height: 9px;
          background-color: #0045B0;
          border-radius: 100%;
          transform: scale(0.75);
          transform-origin: 50% 50%;
          opacity: 0.3;
          animation: antSpinMove 1s infinite linear alternate;
        }

        .ant-spin-dot-item:nth-child(1) {
          top: 0;
          left: 0;
        }

        .ant-spin-dot-item:nth-child(2) {
          top: 0;
          right: 0;
          animation-delay: 0.4s;
        }

        .ant-spin-dot-item:nth-child(3) {
          right: 0;
          bottom: 0;
          animation-delay: 0.8s;
        }

        .ant-spin-dot-item:nth-child(4) {
          bottom: 0;
          left: 0;
          animation-delay: 1.2s;
        }

        @keyframes antRotate {
          to {
            transform: rotate(405deg);
          }
        }

        @keyframes antSpinMove {
          to {
            opacity: 1;
          }
        }

        .ant-spin-text {
          margin-top: 5px;
          font-size: 14px;
          text-align: center;
        }
      </style>
      <div class="ant-spin-spinning-wrap">
        <div class="ant-spin-spinning">
          <span class="ant-spin-dot-holder">
            <span class="ant-spin-dot-spin">
              <i class="ant-spin-dot-item"></i>
              <i class="ant-spin-dot-item"></i>
              <i class="ant-spin-dot-item"></i>
              <i class="ant-spin-dot-item"></i>
            </span>
          </span>
          <div class="ant-spin-text">Loading...</div>
        </div>
      </div>
    `;
  }
})();

