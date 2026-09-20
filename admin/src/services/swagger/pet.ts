/* eslint-disable */
import request from '@/utils/request';

/** Update an existing pet PUT /pet */
export async function updatePet(body: API.Pet, options?: Record<string, unknown>) {
  return request<unknown>('/pet', {
    method: 'PUT',
    headers: {
      'Content-Type': 'application/json',
    },
    data: body,
    ...(options || {}),
  });
}

/** Add a new pet to the store POST /pet */
export async function addPet(body: API.Pet, options?: Record<string, unknown>) {
  return request<unknown>('/pet', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    data: body,
    ...(options || {}),
  });
}

/** Find pet by ID Returns a single pet GET /pet/${param0} */
export async function getPetById(
  // 叠加生成的Param类型 (非body参数swagger默认没有生成对象)
  params: API.getPetByIdParams,
  options?: Record<string, unknown>,
) {
  const { petId: param0, ...queryParams } = params;
  return request<API.Pet>(`/pet/${param0}`, {
    method: 'GET',
    params: { ...queryParams },
    ...(options || {}),
  });
}

/** Updates a pet in the store with form data POST /pet/${param0} */
export async function updatePetWithForm(
  // 叠加生成的Param类型 (非body参数swagger默认没有生成对象)
  params: API.updatePetWithFormParams,
  body: { name?: string; status?: string },
  options?: Record<string, unknown>,
) {
  const { petId: param0, ...queryParams } = params;
  const formData = new FormData();

  Object.keys(body).forEach((ele) => {
    const item = (body as Record<string, unknown>)[ele];

    if (item !== undefined && item !== null) {
      formData.append(
        ele,
        typeof item === 'object' && !(item instanceof Blob)
          ? JSON.stringify(item)
          : (item as string | Blob),
      );
    }
  });

  return request<unknown>(`/pet/${param0}`, {
    method: 'POST',
    params: { ...queryParams },
    data: formData,
    ...(options || {}),
  });
}

/** Deletes a pet DELETE /pet/${param0} */
export async function deletePet(
  // 叠加生成的Param类型 (非body参数swagger默认没有生成对象)
  params: API.deletePetParams & {
    // header
    api_key?: string;
  },
  options?: Record<string, unknown>,
) {
  const { petId: param0, ...queryParams } = params;
  return request<unknown>(`/pet/${param0}`, {
    method: 'DELETE',
    headers: {},
    params: { ...queryParams },
    ...(options || {}),
  });
}

/** uploads an image POST /pet/${param0}/uploadImage */
export async function uploadFile(
  // 叠加生成的Param类型 (非body参数swagger默认没有生成对象)
  params: API.uploadFileParams,
  body: { additionalMetadata?: string; file?: string },
  file?: File,
  options?: Record<string, unknown>,
) {
  const { petId: param0, ...queryParams } = params;
  const formData = new FormData();

  if (file) {
    formData.append('file', file);
  }

  Object.keys(body).forEach((ele) => {
    const item = (body as Record<string, unknown>)[ele];

    if (item !== undefined && item !== null) {
      formData.append(
        ele,
        typeof item === 'object' && !(item instanceof Blob)
          ? JSON.stringify(item)
          : (item as string | Blob),
      );
    }
  });

  return request<API.ApiResponse>(`/pet/${param0}/uploadImage`, {
    method: 'POST',
    params: { ...queryParams },
    data: formData,
    requestType: 'form',
    ...(options || {}),
  });
}

/** Finds Pets by status Multiple status values can be provided with comma separated strings GET /pet/findByStatus */
export async function findPetsByStatus(
  // 叠加生成的Param类型 (非body参数swagger默认没有生成对象)
  params: API.findPetsByStatusParams,
  options?: Record<string, unknown>,
) {
  return request<API.Pet[]>('/pet/findByStatus', {
    method: 'GET',
    params: {
      ...params,
    },
    ...(options || {}),
  });
}

/** Finds Pets by tags Muliple tags can be provided with comma separated strings. Use         tag1, tag2, tag3 for testing. GET /pet/findByTags */
export async function findPetsByTags(
  // 叠加生成的Param类型 (非body参数swagger默认没有生成对象)
  params: API.findPetsByTagsParams,
  options?: Record<string, unknown>,
) {
  return request<API.Pet[]>('/pet/findByTags', {
    method: 'GET',
    params: {
      ...params,
    },
    ...(options || {}),
  });
}
