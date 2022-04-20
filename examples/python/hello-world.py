from azul import *

class DataModel:
    def __init__(self, counter):
        self.counter = counter

# model -> view
def my_layout_func(data, info):

    label = Dom.text("{}".format(data.counter))
    label.set_inline_style("font-size: 50px")

    button = Button("Update counter")
    button.set_on_click(data, my_on_click)
    button = button.dom()
    button.set_inline_style("flex-grow:1")

    root = Dom.body()
    root.add_child(label)
    root.add_child(button)

    return root.style(Css.empty())

# model <- view
def my_on_click(data, info):
    data.counter += 1;
    return Update.RefreshDom

model = DataModel(5)
app = App(model, AppConfig(LayoutSolver.Default))
app.run(WindowCreateOptions(my_layout_func))