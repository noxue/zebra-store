import type { ExtraMessages } from '../index'

// Catalog group (商品 / 卡密) strings that were inline text maps in the original ProductEditModal.vue.
const messages: ExtraMessages = {
  'zh-CN': {
    admin: {
      products: {
        extra: {
          categoryLeafTip: '已有二级分类的一级分类不能直接挂商品，请选择末级分类',
          categoryRequired: '请选择商品分类',
        },
      },
    },
  },
  'zh-TW': {
    admin: {
      products: {
        extra: {
          categoryLeafTip: '已有二級分類的一級分類不能直接掛商品，請選擇末級分類',
          categoryRequired: '請選擇商品分類',
        },
      },
    },
  },
  'en-US': {
    admin: {
      products: {
        extra: {
          categoryLeafTip: 'Root categories with child categories cannot receive products directly. Choose a leaf category.',
          categoryRequired: 'Please select a category',
        },
      },
    },
  },
}

export default messages
