# DOM-Tree-rs

A Rust library to convert plain HTML into a DOM Tree Data Structure. Takes block of HTML code processes char by char and stores it into `struct Node`.

For example if the HTML body is,

```
<ul id='list'> 
  <li class='item'> Item 1</li> 
</ul>
```

The library will convert it to,

```
Node {
    tag_name: "ul",
    children: [
        Node {
            tag_name: "li",
            children: [
                Node {
                    tag_name: "",
                    children: [],
                    props: Text(
                        "Item 1",
                    ),
                },
            ],
            props: Data(
                NodeData {
                    attrs: {
                        "class": "item",
                    },
                },
            ),
        },
    ],
    props: Data(
        NodeData {
            attrs: {
                "id": "list",
            },
        },
    ),
}
```