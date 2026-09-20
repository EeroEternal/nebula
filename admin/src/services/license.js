import request from '../utils/request';
// 获取证书
export async function getLicense() {
  return request(`/license`, {
    method: 'GET',
  });
}
// 设置证书
export async function updateLicense(key) {
  return request(`/license`, {
    method: 'POST',
    data: {
      license_key: key
    },
  });
}
